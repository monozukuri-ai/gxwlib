//! Ordered condition DAG. Every contact represents one instruction's read;
//! sharing a condition never performs that read again after an output write.
use crate::{
    GxwError, InstructionProgram, Opcode,
    analysis::{AnalysisOptions, Finding, limit},
};
use serde::Serialize;
use std::sync::Arc;
pub mod render;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ConditionKind {
    Contact,
    And,
    Or,
}
#[derive(Debug, Clone, Serialize)]
pub struct Condition {
    pub id: usize,
    pub kind: ConditionKind,
    pub instruction: usize,
    pub inputs: Vec<usize>,
    pub inverted: bool,
}
#[derive(Debug, Clone, Serialize)]
pub struct LadderOutput {
    pub instruction: usize,
    pub condition: usize,
}
#[derive(Debug, Clone, Serialize)]
pub struct LadderControl {
    pub instruction: usize,
    pub condition: Option<usize>,
}
#[derive(Debug, Serialize)]
pub struct LadderGraph {
    pub schema_version: u32,
    pub source_sha256: String,
    pub complete: bool,
    pub conditions: Vec<Condition>,
    pub outputs: Vec<LadderOutput>,
    pub controls: Vec<LadderControl>,
    pub processed_instructions: usize,
    pub diagnostics: Vec<Finding>,
    #[serde(skip)]
    pub program: Arc<InstructionProgram>,
}
impl LadderGraph {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    fn condition(
        &mut self,
        kind: ConditionKind,
        instruction: usize,
        inputs: Vec<usize>,
        inverted: bool,
    ) -> usize {
        let id = self.conditions.len();
        self.conditions.push(Condition {
            id,
            kind,
            instruction,
            inputs,
            inverted,
        });
        id
    }
    fn finding(&mut self, index: Option<usize>, code: &str, message: &str) {
        self.diagnostics
            .push(Finding::at(&self.program, index, code, "warning", message));
    }
}
#[derive(Clone, Copy)]
struct ResultValue {
    condition: usize,
    output_seen: bool,
}

pub fn build(
    program: Arc<InstructionProgram>,
    options: &AnalysisOptions,
) -> Result<LadderGraph, GxwError> {
    limit(
        "ladder instructions",
        program.instructions.len(),
        options.max_instructions,
    )?;
    if options.max_block_depth == 0 || options.max_mps_depth == 0 {
        return Err(GxwError::format("stack limits", "depth must be positive"));
    }
    let mut graph = LadderGraph {
        schema_version: 1,
        source_sha256: program.source_sha256.clone(),
        complete: false,
        conditions: Vec::new(),
        outputs: Vec::new(),
        controls: Vec::new(),
        processed_instructions: 0,
        diagnostics: Vec::new(),
        program: program.clone(),
    };
    let mut current: Option<ResultValue> = None;
    let mut blocks = Vec::<ResultValue>::new();
    let mut saved = Vec::<usize>::new();
    for (index, ins) in program.instructions.iter().enumerate() {
        if !ins.supported || !crate::ir::valid_instruction(ins) {
            graph.finding(Some(index), "GXW_LADDER_UNSUPPORTED", "Unknown effects stop ladder reconstruction; subsequent instructions remain in the source list");
            break;
        }
        let op = ins.opcode.expect("validated instruction");
        if !matches!(op, Opcode::Ld | Opcode::Ldi | Opcode::End) && current.is_none() {
            graph.finding(
                Some(index),
                "GXW_LOGIC_UNDERFLOW",
                "Instruction needs a preceding logic result",
            );
            break;
        }
        match op {
            Opcode::Ld | Opcode::Ldi => {
                if let Some(previous) = current {
                    // OUT does not erase the logic result: the manual's cascade
                    // example uses OUT; LD; OR; ANB. Older completed results may
                    // roll out of the bounded logic stack between independent rungs.
                    if blocks.len() + 1 >= options.max_block_depth {
                        if blocks.first().is_some_and(|b| b.output_seen) {
                            blocks.remove(0);
                        } else {
                            graph.finding(
                                Some(index),
                                "GXW_BLOCK_OVERFLOW",
                                "Uncombined logic exceeds the selected block-stack depth",
                            );
                            break;
                        }
                    }
                    blocks.push(previous);
                }
                let condition =
                    graph.condition(ConditionKind::Contact, index, Vec::new(), op == Opcode::Ldi);
                current = Some(ResultValue {
                    condition,
                    output_seen: false,
                });
            }
            Opcode::And | Opcode::Ani | Opcode::Or | Opcode::Ori => {
                let contact = graph.condition(
                    ConditionKind::Contact,
                    index,
                    Vec::new(),
                    matches!(op, Opcode::Ani | Opcode::Ori),
                );
                let condition = graph.condition(
                    if matches!(op, Opcode::And | Opcode::Ani) {
                        ConditionKind::And
                    } else {
                        ConditionKind::Or
                    },
                    index,
                    vec![current.unwrap().condition, contact],
                    false,
                );
                current = Some(ResultValue {
                    condition,
                    output_seen: false,
                });
            }
            Opcode::Anb | Opcode::Orb => {
                let Some(previous) = blocks.pop() else {
                    graph.finding(
                        Some(index),
                        "GXW_BLOCK_UNDERFLOW",
                        "ANB/ORB needs two preceding circuit blocks",
                    );
                    break;
                };
                let condition = graph.condition(
                    if op == Opcode::Anb {
                        ConditionKind::And
                    } else {
                        ConditionKind::Or
                    },
                    index,
                    vec![previous.condition, current.unwrap().condition],
                    false,
                );
                current = Some(ResultValue {
                    condition,
                    output_seen: false,
                });
                graph.controls.push(LadderControl {
                    instruction: index,
                    condition: Some(condition),
                });
            }
            Opcode::Mps => {
                if saved.len() >= options.max_mps_depth {
                    graph.finding(
                        Some(index),
                        "GXW_MPS_OVERFLOW",
                        "MPS exceeds the selected saved-result stack depth",
                    );
                    break;
                }
                saved.push(current.unwrap().condition);
                graph.controls.push(LadderControl {
                    instruction: index,
                    condition: current.map(|v| v.condition),
                });
            }
            Opcode::Mrd | Opcode::Mpp => {
                let Some(condition) = (if op == Opcode::Mpp {
                    saved.pop()
                } else {
                    saved.last().copied()
                }) else {
                    graph.finding(
                        Some(index),
                        "GXW_MPS_UNDERFLOW",
                        "MRD/MPP has no saved result",
                    );
                    break;
                };
                current = Some(ResultValue {
                    condition,
                    output_seen: false,
                });
                graph.controls.push(LadderControl {
                    instruction: index,
                    condition: Some(condition),
                });
            }
            Opcode::Out | Opcode::Set | Opcode::Rst => {
                graph.outputs.push(LadderOutput {
                    instruction: index,
                    condition: current.unwrap().condition,
                });
                current.as_mut().unwrap().output_seen = true;
            }
            Opcode::End => {
                graph.controls.push(LadderControl {
                    instruction: index,
                    condition: None,
                });
                graph.processed_instructions = index + 1;
                if index + 1 != program.instructions.len() {
                    graph.finding(
                        Some(index),
                        "GXW_EARLY_END",
                        "Instructions after END are not reconstructed",
                    );
                }
                break;
            }
        }
        graph.processed_instructions = index + 1;
    }
    if !saved.is_empty() {
        graph.finding(None, "GXW_MPS_UNCLOSED", "Saved results remain without matching MPP; later opaque code may also affect the stack");
    }
    if blocks.iter().any(|b| !b.output_seen) || current.is_some_and(|v| !v.output_seen) {
        graph.finding(
            None,
            "GXW_BLOCK_UNCLOSED",
            "Unconsumed circuit logic remains; connections are incomplete",
        );
    }
    if !program.complete {
        graph.finding(
            None,
            "GXW_ANALYSIS_PARTIAL",
            "Input contains unsupported syntax or opaque regions; overall analysis is incomplete",
        );
    }
    graph.complete = program.complete
        && graph.processed_instructions == program.instructions.len()
        && graph.diagnostics.is_empty();
    Ok(graph)
}
