//! Instruction diff with bounded LCS memory. No execution-equivalence claim.
use crate::{GxwError, InstructionProgram, analysis::limit};
use serde::Serialize;
use std::sync::Arc;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DiffKind {
    Equal,
    Insert,
    Delete,
    Replace,
}
#[derive(Debug, Clone, Serialize)]
pub struct DiffHunk {
    pub kind: DiffKind,
    pub left_start: usize,
    pub left_count: usize,
    pub right_start: usize,
    pub right_count: usize,
}
#[derive(Debug, Serialize)]
pub struct ProgramDiff {
    pub schema_version: u32,
    pub left_sha256: String,
    pub right_sha256: String,
    pub complete: bool,
    pub different: bool,
    pub opaque_changed: bool,
    pub hunks: Vec<DiffHunk>,
    #[serde(skip)]
    pub left: Arc<InstructionProgram>,
    #[serde(skip)]
    pub right: Arc<InstructionProgram>,
}
impl ProgramDiff {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
}
fn keys(p: &InstructionProgram) -> Vec<String> {
    p.instructions
        .iter()
        .map(|i| {
            if i.supported && crate::ir::valid_instruction(i) {
                serde_json::json!([
                    i.opcode,
                    i.operands.iter().map(|o| &o.value).collect::<Vec<_>>()
                ])
                .to_string()
            } else {
                // Unknown text/raw records can be compared for change, but never
                // make an incomplete program into a semantic equality result.
                serde_json::json!([
                    "unresolved",
                    i.mnemonic,
                    i.original_mnemonic,
                    i.operands
                        .iter()
                        .map(|o| (&o.value, &o.original_text))
                        .collect::<Vec<_>>(),
                    p.source_bytes(&i.source)
                ])
                .to_string()
            }
        })
        .collect()
}
fn push(hunks: &mut Vec<DiffHunk>, kind: DiffKind, l: usize, r: usize, lc: usize, rc: usize) {
    if lc + rc == 0 {
        return;
    }
    if let Some(last) = hunks.last_mut()
        && last.kind == kind
        && last.left_start + last.left_count == l
        && last.right_start + last.right_count == r
    {
        last.left_count += lc;
        last.right_count += rc;
        return;
    }
    hunks.push(DiffHunk {
        kind,
        left_start: l,
        right_start: r,
        left_count: lc,
        right_count: rc,
    });
}
pub fn diff_programs(
    left: Arc<InstructionProgram>,
    right: Arc<InstructionProgram>,
    max_cells: usize,
) -> Result<ProgramDiff, GxwError> {
    limit(
        "diff input instructions",
        left.instructions
            .len()
            .saturating_add(right.instructions.len()),
        200_000,
    )?;
    let a = keys(&left);
    let b = keys(&right);
    let prefix = a.iter().zip(&b).take_while(|(a, b)| a == b).count();
    let suffix = a[prefix..]
        .iter()
        .rev()
        .zip(b[prefix..].iter().rev())
        .take_while(|(a, b)| a == b)
        .count();
    let n = a.len() - prefix - suffix;
    let m = b.len() - prefix - suffix;
    let mut hunks = Vec::new();
    push(&mut hunks, DiffKind::Equal, 0, 0, prefix, prefix);
    if n == 0 || m == 0 {
        push(
            &mut hunks,
            if n == 0 {
                DiffKind::Insert
            } else {
                DiffKind::Delete
            },
            prefix,
            prefix,
            n,
            m,
        );
    } else {
        let cells = (n + 1)
            .checked_mul(m + 1)
            .ok_or_else(|| GxwError::format("diff", "cell count overflow"))?;
        limit("diff LCS cells", cells, max_cells)?;
        let stride = m + 1;
        let mut table = vec![0u32; cells];
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                table[i * stride + j] = if a[prefix + i] == b[prefix + j] {
                    1 + table[(i + 1) * stride + j + 1]
                } else {
                    table[(i + 1) * stride + j].max(table[i * stride + j + 1])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && a[prefix + i] == b[prefix + j] {
                push(&mut hunks, DiffKind::Equal, prefix + i, prefix + j, 1, 1);
                i += 1;
                j += 1;
            } else if i < n && (j == m || table[(i + 1) * stride + j] >= table[i * stride + j + 1])
            {
                push(&mut hunks, DiffKind::Delete, prefix + i, prefix + j, 1, 0);
                i += 1;
            } else {
                push(&mut hunks, DiffKind::Insert, prefix + i, prefix + j, 0, 1);
                j += 1;
            }
        }
    }
    push(
        &mut hunks,
        DiffKind::Equal,
        a.len() - suffix,
        b.len() - suffix,
        suffix,
        suffix,
    );
    let mut merged: Vec<DiffHunk> = Vec::new();
    for h in hunks {
        if h.kind != DiffKind::Equal
            && let Some(last) = merged.last_mut()
            && last.kind != DiffKind::Equal
        {
            last.left_count += h.left_count;
            last.right_count += h.right_count;
            last.kind = if last.left_count > 0 && last.right_count > 0 {
                DiffKind::Replace
            } else {
                last.kind
            };
        } else {
            merged.push(h);
        }
    }
    // Opaque bytes are not in the instruction list: include changes there too.
    let opaque_a: Vec<_> = left
        .opaque_regions
        .iter()
        .map(|s| left.source_bytes(s))
        .collect();
    let opaque_b: Vec<_> = right
        .opaque_regions
        .iter()
        .map(|s| right.source_bytes(s))
        .collect();
    let opaque_changed = opaque_a != opaque_b;
    let different = merged.iter().any(|h| h.kind != DiffKind::Equal) || opaque_changed;
    Ok(ProgramDiff {
        schema_version: 1,
        left_sha256: left.source_sha256.clone(),
        right_sha256: right.source_sha256.clone(),
        complete: left.complete
            && right.complete
            && left
                .instructions
                .iter()
                .chain(&right.instructions)
                .all(crate::ir::valid_instruction),
        different,
        opaque_changed,
        hunks: merged,
        left,
        right,
    })
}
