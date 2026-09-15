//! Deterministic, bounded SVG output-condition layouts. Contact occurrences
//! retain their original read instruction even when a saved result is reused.
use super::{ConditionKind, LadderGraph};
use crate::{
    GxwError, InstructionSpan, Opcode, Operand, OperandValue,
    analysis::{DeviceRef, limit},
};
use serde::Serialize;
use std::{fmt::Write, sync::Arc};

const CW: usize = 180;
const CH: usize = 90;
#[derive(Debug, Clone)]
pub struct RenderOptions {
    pub max_elements: usize,
    pub max_output_bytes: usize,
}
impl Default for RenderOptions {
    fn default() -> Self {
        Self {
            max_elements: 20_000,
            max_output_bytes: 16 << 20,
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct SvgElement {
    pub id: String,
    pub kind: String,
    pub instruction: usize,
    pub source: InstructionSpan,
    pub label: String,
    pub inverted: bool,
    pub x: usize,
    pub y: usize,
    pub width: usize,
    pub height: usize,
}
#[derive(Debug, Serialize)]
pub struct SvgDocument {
    pub schema_version: u32,
    pub source_sha256: String,
    pub layout: String,
    pub complete: bool,
    pub width: usize,
    pub height: usize,
    pub elements: Vec<SvgElement>,
    #[serde(skip)]
    pub svg: String,
    #[serde(skip)]
    pub graph: Arc<LadderGraph>,
    #[serde(skip)]
    max_output_bytes: usize,
}
pub(crate) fn escape(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}
pub fn operand_label(operand: &Operand) -> String {
    match &operand.value {
        OperandValue::Device {
            device, address, ..
        } => DeviceRef {
            device: *device,
            address: *address,
        }
        .name(),
        OperandValue::Constant {
            value, radix: 16, ..
        } => format!("H{value:X}"),
        OperandValue::Constant { value, .. } => format!("K{value}"),
        OperandValue::Unknown => operand
            .original_text
            .as_deref()
            .map(short)
            .unwrap_or_else(|| "<?>".into()),
    }
}
fn short(s: &str) -> String {
    let mut value: String = s.chars().take(100).collect();
    if s.chars().nth(100).is_some() {
        value.push('…');
    }
    value
}
fn line(out: &mut String, x1: usize, y1: usize, x2: usize, y2: usize) {
    write!(out, "<path class=\"wire\" d=\"M{x1} {y1}H{x2}V{y2}\"/>").unwrap();
}
fn symbol(
    doc: &mut SvgDocument,
    out: &mut String,
    instruction: usize,
    kind: &str,
    label: String,
    inverted: bool,
    position: (usize, usize),
) {
    let (x, y) = position;
    let ins = &doc.graph.program.instructions[instruction];
    let id = format!("gxw-e{}", doc.elements.len());
    let width = if kind == "control" { 150 } else { CW };
    let height = if kind == "control" { 38 } else { CH };
    let source = ins.source.clone();
    write!(out, "<g id=\"{id}\" class=\"gxw-symbol {kind}\" data-instruction=\"{instruction}\" data-source-id=\"{}\" data-offset=\"{}\" data-length=\"{}\" tabindex=\"0\" role=\"button\" aria-label=\"{} instruction {instruction}\"><title>{} · instruction {instruction}</title><rect class=\"hit\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" rx=\"8\"/>",
        source.source_id.map(|id| id.to_string()).unwrap_or_else(|| "csv".into()), source.offset, source.length,
        escape(&label), escape(&label), x+4, y+4, width-8, height-8).unwrap();
    if kind == "control" {
        write!(
            out,
            "<text x=\"{}\" y=\"{}\" class=\"control-label\">#{} {}</text>",
            x + 12,
            y + 25,
            instruction,
            escape(&label)
        )
        .unwrap();
    } else {
        let cy = y + CH / 2;
        let half_gap = if kind == "contact" { 10 } else { 19 };
        line(out, x, cy, x + CW / 2 - half_gap, cy);
        line(out, x + CW / 2 + half_gap, cy, x + CW, cy);
        if kind == "contact" {
            write!(
                out,
                "<path class=\"glyph\" d=\"M{} {}v28M{} {}v28\"/>",
                x + CW / 2 - 10,
                cy - 14,
                x + CW / 2 + 10,
                cy - 14
            )
            .unwrap();
            if inverted {
                write!(
                    out,
                    "<path class=\"glyph\" d=\"M{} {}l28 -34\"/>",
                    x + CW / 2 - 14,
                    cy + 17
                )
                .unwrap();
            }
        } else {
            write!(
                out,
                "<path class=\"glyph\" d=\"M{} {}q-18 18 0 36M{} {}q18 18 0 36\"/>",
                x + CW / 2 - 10,
                cy - 18,
                x + CW / 2 + 10,
                cy - 18
            )
            .unwrap();
            let marker = match ins.opcode {
                Some(Opcode::Set) => "S",
                Some(Opcode::Rst) => "R",
                _ => "",
            };
            write!(
                out,
                "<text class=\"coil-marker\" x=\"{}\" y=\"{}\">{marker}</text>",
                x + CW / 2,
                cy + 5
            )
            .unwrap();
        }
        write!(out,"<text class=\"device-label\" x=\"{}\" y=\"{}\">{}</text><text class=\"instruction-label\" x=\"{}\" y=\"{}\">#{} {}</text>",x+CW/2,y+17,escape(&label),x+CW/2,y+77,instruction,ins.opcode.map(|op|op.as_str()).unwrap_or("?")).unwrap();
    }
    out.push_str("</g>");
    doc.elements.push(SvgElement {
        id,
        kind: kind.into(),
        instruction,
        source,
        label,
        inverted,
        x,
        y,
        width,
        height,
    });
}

pub fn render_svg(
    graph: Arc<LadderGraph>,
    options: &RenderOptions,
) -> Result<SvgDocument, GxwError> {
    limit("render element limit", options.max_elements, 1_000_000)?;
    limit("render output limit", options.max_output_bytes, 256 << 20)?;
    let mut sizes = Vec::<(usize, usize, usize)>::new();
    for (index, node) in graph.conditions.iter().enumerate() {
        if node.id != index || node.instruction >= graph.program.instructions.len() {
            return Err(GxwError::format(
                "ladder graph",
                "invalid condition identity",
            ));
        }
        let size = if node.kind == ConditionKind::Contact {
            if !node.inputs.is_empty() {
                return Err(GxwError::format(
                    "ladder graph",
                    "contact has child conditions",
                ));
            }
            (1, 1, 1)
        } else {
            let [a, b] = node.inputs.as_slice() else {
                return Err(GxwError::format(
                    "ladder graph",
                    "binary condition needs two inputs",
                ));
            };
            let &(wa, ha, ca) = sizes
                .get(*a)
                .ok_or_else(|| GxwError::format("ladder graph", "invalid condition reference"))?;
            let &(wb, hb, cb) = sizes
                .get(*b)
                .ok_or_else(|| GxwError::format("ladder graph", "invalid condition reference"))?;
            if node.kind == ConditionKind::And {
                (wa + wb, ha.max(hb), ca + cb)
            } else {
                (wa.max(wb), ha + hb, ca + cb)
            }
        };
        limit("expanded condition cells", size.2, options.max_elements)?;
        sizes.push(size);
    }
    let mut cells = graph.controls.len();
    let mut width = 960;
    let mut height = 135;
    if graph
        .outputs
        .iter()
        .any(|o| o.instruction >= graph.program.instructions.len())
        || graph.controls.iter().any(|c| {
            c.instruction >= graph.program.instructions.len()
                || c.condition.is_some_and(|id| id >= sizes.len())
        })
    {
        return Err(GxwError::format(
            "ladder graph",
            "invalid output/control reference",
        ));
    }
    for output in &graph.outputs {
        let &(w, h, count) = sizes
            .get(output.condition)
            .ok_or_else(|| GxwError::format("ladder graph", "invalid output condition"))?;
        cells += count + 1;
        limit("rendered elements", cells, options.max_elements)?;
        width = width.max((w + 1) * CW + 100);
        height += h * CH + 58;
    }
    limit("rendered elements", cells, options.max_elements)?;
    height += graph.controls.len().div_ceil(5) * 48 + 65;
    let mut doc = SvgDocument {
        schema_version: 1,
        source_sha256: graph.source_sha256.clone(),
        layout: "output_conditions_relaid".into(),
        complete: graph.complete,
        width,
        height,
        elements: Vec::new(),
        svg: String::new(),
        graph,
        max_output_bytes: options.max_output_bytes,
    };
    let mut body = format!(
        "<rect width=\"100%\" height=\"100%\" fill=\"#f6f8fc\"/><text x=\"32\" y=\"35\" class=\"title\">{}</text><text x=\"32\" y=\"62\" class=\"subtitle\">Reconstructed ladder · output conditions · {} </text><text x=\"32\" y=\"88\" class=\"subtitle\"># identifies the original read/write instruction; repeated contacts refer to the same evaluated result.</text>",
        escape(&short(
            doc.graph
                .program
                .logical_name
                .as_deref()
                .unwrap_or("Ladder")
        )),
        if doc.complete {
            "complete supported structure"
        } else {
            "PARTIAL — unresolved structure remains"
        }
    );
    if doc.graph.outputs.is_empty() {
        body.push_str("<text x=\"32\" y=\"125\" class=\"subtitle\">No resolved outputs. Inspect the instruction list and diagnostics.</text>");
    }
    let mut y = 120;
    for output_index in 0..doc.graph.outputs.len() {
        let output = doc.graph.outputs[output_index].clone();
        let (w, h, _) = sizes[output.condition];
        let x = 48;
        let top = y + 36;
        write!(body,"<rect class=\"panel\" x=\"24\" y=\"{y}\" width=\"{}\" height=\"{}\" rx=\"12\"/><text class=\"section-label\" x=\"40\" y=\"{}\">Output {} · instruction #{}</text>",width-48,h*CH+44,y+24,output_index+1,output.instruction).unwrap();
        let mut tasks = vec![(output.condition, x, top)];
        while let Some((id, nx, ny)) = tasks.pop() {
            let node = doc.graph.conditions[id].clone();
            match node.kind {
                ConditionKind::Contact => {
                    let ins = doc
                        .graph
                        .program
                        .instructions
                        .get(node.instruction)
                        .ok_or_else(|| {
                            GxwError::format("ladder graph", "invalid instruction reference")
                        })?;
                    let operand = ins.operands.first().ok_or_else(|| {
                        GxwError::format("ladder graph", "missing contact operand")
                    })?;
                    let label = operand_label(operand);
                    symbol(
                        &mut doc,
                        &mut body,
                        node.instruction,
                        "contact",
                        label,
                        node.inverted,
                        (nx, ny),
                    );
                }
                ConditionKind::And => {
                    let (a, b) = (node.inputs[0], node.inputs[1]);
                    tasks.push((b, nx + sizes[a].0 * CW, ny));
                    tasks.push((a, nx, ny));
                }
                ConditionKind::Or => {
                    let (a, b) = (node.inputs[0], node.inputs[1]);
                    let end = nx + sizes[id].0 * CW;
                    let lower = ny + sizes[a].1 * CH;
                    line(&mut body, nx, ny + CH / 2, nx, lower + CH / 2);
                    line(
                        &mut body,
                        nx + sizes[a].0 * CW,
                        ny + CH / 2,
                        end,
                        ny + CH / 2,
                    );
                    line(
                        &mut body,
                        nx + sizes[b].0 * CW,
                        lower + CH / 2,
                        end,
                        lower + CH / 2,
                    );
                    line(&mut body, end, ny + CH / 2, end, lower + CH / 2);
                    tasks.push((b, nx, lower));
                    tasks.push((a, nx, ny));
                }
            }
        }
        let label = doc.graph.program.instructions[output.instruction]
            .operands
            .iter()
            .map(operand_label)
            .collect::<Vec<_>>()
            .join(" ");
        symbol(
            &mut doc,
            &mut body,
            output.instruction,
            "coil",
            label,
            false,
            (x + w * CW, top),
        );
        line(&mut body, x, top + CH / 2 - 24, x, top + h * CH - 20);
        line(
            &mut body,
            x + (w + 1) * CW,
            top + CH / 2 - 24,
            x + (w + 1) * CW,
            top + CH / 2 + 24,
        );
        y += h * CH + 58;
        limit("SVG bytes", body.len(), options.max_output_bytes)?;
    }
    for i in 0..doc.graph.controls.len() {
        let instruction = doc.graph.controls[i].instruction;
        let label = doc.graph.program.instructions[instruction]
            .opcode
            .map(|op| op.as_str())
            .unwrap_or("?")
            .to_string();
        symbol(
            &mut doc,
            &mut body,
            instruction,
            "control",
            label,
            false,
            (32 + (i % 5) * 170, y + (i / 5) * 48),
        );
    }
    let style = "<style>.wire,.glyph{fill:none;stroke:#334761;stroke-width:2.2}.panel{fill:white;stroke:#dce3ee}.title{font:600 22px system-ui;fill:#182b45}.subtitle{font:13px system-ui;fill:#53657e}.section-label{font:600 12px system-ui;fill:#53657e}.device-label{font:600 14px ui-monospace,monospace;text-anchor:middle;fill:#163556}.instruction-label{font:11px ui-monospace,monospace;text-anchor:middle;fill:#60758f}.coil-marker{font:13px system-ui;text-anchor:middle;fill:#163556}.control-label{font:12px ui-monospace,monospace;fill:#334761}.hit{fill:transparent;stroke:transparent}.control .hit{fill:#e7edf7}.gxw-symbol{cursor:pointer;outline:none}.gxw-symbol:hover .hit,.gxw-symbol:focus .hit,.gxw-symbol.selected .hit{fill:#dbeafe;fill-opacity:.65;stroke:#2563eb}.gxw-symbol.selected .glyph{stroke:#2563eb}</style>";
    let metadata = escape(
        &doc.to_json()
            .map_err(|e| GxwError::format("SVG metadata", e))?,
    );
    doc.svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"Reconstructed ladder\"><metadata>{metadata}</metadata>{style}{body}</svg>"
    );
    limit("SVG bytes", doc.svg.len(), options.max_output_bytes)?;
    Ok(doc)
}
impl SvgDocument {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    pub fn to_html(&self) -> Result<String, GxwError> {
        let program = &self.graph.program;
        let instructions: Vec<_> = program.instructions.iter().enumerate().map(|(index, ins)| {
            let bytes = program.source_bytes(&ins.source).unwrap_or_default();
            serde_json::json!({"instruction":index,"step":ins.step,"mnemonic":short(ins.mnemonic.as_deref().unwrap_or("<?>")),
                "operands":ins.operands.iter().map(operand_label).collect::<Vec<_>>(),"source":ins.source,"supported":ins.supported,
                "bytes_preview":bytes.iter().take(64).map(|b|format!("{b:02x}")).collect::<Vec<_>>().join(" "),"preview_truncated":bytes.len()>64})
        }).collect();
        let data = serde_json::json!({"name":program.logical_name,"sha256":program.source_sha256,"origin":program.origin,
            "complete":self.complete,"processed_instructions":self.graph.processed_instructions,"instructions":instructions,
            "diagnostics":self.graph.diagnostics,"input_diagnostics":program.diagnostics,"opaque_regions":program.opaque_regions});
        let json = data
            .to_string()
            .replace('<', "\\u003c")
            .replace('&', "\\u0026");
        // Substitute only template markers, never marker text from user data.
        let (before, rest) = include_str!("viewer.html").split_once("{{SVG}}").unwrap();
        let (middle, after) = rest.split_once("{{DATA}}").unwrap();
        let html = format!("{before}{}{middle}{json}{after}", self.svg);
        limit("HTML bytes", html.len(), self.max_output_bytes)?;
        Ok(html)
    }
}
