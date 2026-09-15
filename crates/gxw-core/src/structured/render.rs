//! Native block geometry, stacked for browsing. Coordinates are not scan order.
use super::*;
use crate::{RenderOptions, ladder::render::escape};
use std::fmt::Write;

#[derive(Debug, Serialize)]
pub struct StructuredSvgElement {
    pub id: String,
    pub block: usize,
    pub kind: String,
    pub label: String,
    pub source: SourceSpan,
}
#[derive(Debug, Serialize)]
pub struct StructuredSvgDocument {
    pub schema_version: u32,
    pub source_sha256: String,
    pub layout: String,
    pub structure_complete: bool,
    pub width: u64,
    pub height: u64,
    pub elements: Vec<StructuredSvgElement>,
    #[serde(skip)]
    pub svg: String,
    #[serde(skip)]
    pub program: Arc<StructuredProgram>,
    #[serde(skip)]
    max_output_bytes: usize,
}
fn begin(
    body: &mut String,
    elements: &mut Vec<StructuredSvgElement>,
    block: usize,
    kind: &str,
    label: &str,
    source: &SourceSpan,
) {
    let id = format!("gxw-s{}", elements.len());
    write!(body,"<g id=\"{id}\" class=\"item {kind}\" data-block=\"{block}\" data-source-id=\"{}\" data-offset=\"{}\" data-length=\"{}\" tabindex=\"0\" role=\"button\" aria-label=\"{}\"><title>{}</title>",source.source_id,source.offset,source.length,escape(label),escape(label)).unwrap();
    elements.push(StructuredSvgElement {
        id,
        block,
        kind: kind.into(),
        label: label.into(),
        source: source.clone(),
    });
}
pub fn render_structured(
    program: Arc<StructuredProgram>,
    options: &RenderOptions,
) -> Result<StructuredSvgDocument, GxwError> {
    let count: usize = program
        .blocks
        .iter()
        .map(|b| {
            b.nodes.iter().map(|n| 1 + n.ports.len()).sum::<usize>()
                + b.wires.len()
                + b.opaque_records.len()
        })
        .sum();
    check(
        "structured SVG elements",
        count as u64,
        options.max_elements as u64,
    )?;
    let mut body = String::from(
        "<text x=\"24\" y=\"28\">Structured POU · stored block geometry · execution not evaluated</text>",
    );
    if !program.structure_complete {
        body.push_str(
            "<text x=\"24\" y=\"50\">PARTIAL: inspect diagnostics and opaque records</text>",
        );
    }
    let mut width = 800u64;
    let mut height = 80u64;
    let mut elements = Vec::new();
    const UNIT: u64 = 28;
    for block in &program.blocks {
        let mut xmax = 0u32;
        let mut ymax = block.canvas_height;
        for node in &block.nodes {
            xmax = xmax.max(node.bounds[2]);
            ymax = ymax.max(node.bounds[3]);
            for port in &node.ports {
                xmax = xmax.max(port.position.x);
                ymax = ymax.max(port.position.y);
            }
        }
        for wire in &block.wires {
            xmax = xmax.max(wire.start.x).max(wire.end.x);
            ymax = ymax.max(wire.start.y).max(wire.end.y);
        }
        // Avoid browser-hostile viewBoxes even for otherwise valid u32 source coordinates.
        check(
            "structured SVG coordinates",
            u64::from(xmax.max(ymax)),
            100_000,
        )?;
        width = width.max(u64::from(xmax) * UNIT + 100);
        let top = height + 28;
        write!(
            body,
            "<text x=\"24\" y=\"{height}\">Block {}</text>",
            block.index
        )
        .unwrap();
        for wire in &block.wires {
            begin(
                &mut body,
                &mut elements,
                block.index,
                "wire",
                "wire",
                &wire.source,
            );
            let x1 = 24 + u64::from(wire.start.x) * UNIT;
            let y1 = top + u64::from(wire.start.y) * UNIT;
            let x2 = 24 + u64::from(wire.end.x) * UNIT;
            let y2 = top + u64::from(wire.end.y) * UNIT;
            // Nonzero hit area for horizontal/vertical and zero-length wires.
            write!(body,"<rect class=\"wire-hit\" x=\"{}\" y=\"{}\" width=\"{}\" height=\"{}\" style=\"fill:transparent;stroke:none\"/>",x1.min(x2)-3,y1.min(y2)-3,x1.abs_diff(x2)+6,y1.abs_diff(y2)+6).unwrap();
            write!(
                body,
                "<path d=\"M{} {}L{} {}\"/></g>",
                24 + u64::from(wire.start.x) * UNIT,
                top + u64::from(wire.start.y) * UNIT,
                24 + u64::from(wire.end.x) * UNIT,
                top + u64::from(wire.end.y) * UNIT
            )
            .unwrap();
            check(
                "structured SVG bytes",
                body.len() as u64,
                options.max_output_bytes as u64,
            )?;
        }
        for node in &block.nodes {
            let label = match &node.type_name {
                Some(t) => format!("{} : {} [opaque FB]", node.symbol, t),
                None => node.symbol.clone(),
            };
            begin(
                &mut body,
                &mut elements,
                block.index,
                &node.kind,
                &label,
                &node.source,
            );
            let x = 24 + u64::from(node.bounds[0]) * UNIT;
            let y = top + u64::from(node.bounds[1]) * UNIT;
            let w = u64::from(node.bounds[2] - node.bounds[0]) * UNIT;
            let h = u64::from(node.bounds[3] - node.bounds[1]) * UNIT;
            write!(body,"<rect x=\"{x}\" y=\"{y}\" width=\"{}\" height=\"{}\"/><text x=\"{}\" y=\"{}\">{}</text>",w.max(1),h.max(1),x+4,y+14,escape(&label)).unwrap();
            let (cx, cy) = (x + w / 2, y + h / 2);
            match node.kind_code {
                3 | 4 => {
                    write!(
                        body,
                        "<path d=\"M{} {}v20M{} {}v20\"/>",
                        cx.saturating_sub(6),
                        cy.saturating_sub(10),
                        cx + 6,
                        cy.saturating_sub(10)
                    )
                    .unwrap();
                    if node.kind_code == 4 {
                        write!(
                            body,
                            "<path d=\"M{} {}l18 -24\"/>",
                            cx.saturating_sub(9),
                            cy + 12
                        )
                        .unwrap();
                    }
                }
                5 => {
                    write!(
                        body,
                        "<text x=\"{}\" y=\"{}\">( )</text>",
                        cx.saturating_sub(8),
                        cy + 6
                    )
                    .unwrap();
                }
                _ => {}
            }
            body.push_str("</g>");
            for port in &node.ports {
                begin(
                    &mut body,
                    &mut elements,
                    block.index,
                    "port",
                    &format!("port {} · kind {}", port.index, port.kind_code),
                    &port.source,
                );
                write!(
                    body,
                    "<circle cx=\"{}\" cy=\"{}\" r=\"4\"/></g>",
                    24 + u64::from(port.position.x) * UNIT,
                    top + u64::from(port.position.y) * UNIT
                )
                .unwrap();
            }
            check(
                "structured SVG bytes",
                body.len() as u64,
                options.max_output_bytes as u64,
            )?;
        }
        height = top + u64::from(ymax) * UNIT + 40;
        for opaque in &block.opaque_records {
            begin(
                &mut body,
                &mut elements,
                block.index,
                "opaque",
                &opaque.reason,
                &opaque.source,
            );
            write!(
                body,
                "<text x=\"24\" y=\"{height}\">Opaque: {}</text></g>",
                escape(&opaque.reason)
            )
            .unwrap();
            height += 24;
        }
        check("structured SVG height", height, 4_000_000)?;
        check(
            "structured SVG bytes",
            body.len() as u64,
            options.max_output_bytes as u64,
        )?;
    }
    let style = "<style>text{font:12px system-ui;fill:#20344c}rect{fill:#f5f8fc;stroke:#8293a8}path{fill:none;stroke:#344862;stroke-width:2}circle{fill:white;stroke:#344862}.wire path{stroke-width:5;stroke-opacity:.7}.item{cursor:pointer}.item:hover,.item:focus,.selected{outline:2px solid #2563eb}.selected path,.selected rect,.selected circle{stroke:#2563eb;stroke-width:3}.opaque text{fill:#9a3412}</style>";
    let svg = format!(
        "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"{width}\" height=\"{height}\" viewBox=\"0 0 {width} {height}\" role=\"img\" aria-label=\"Structured POU\">{style}{body}</svg>"
    );
    check(
        "structured SVG bytes",
        svg.len() as u64,
        options.max_output_bytes as u64,
    )?;
    Ok(StructuredSvgDocument {
        schema_version: 1,
        source_sha256: program.source_sha256.clone(),
        layout: "stored_block_geometry_stacked".into(),
        structure_complete: program.structure_complete,
        width,
        height,
        elements,
        svg,
        program,
        max_output_bytes: options.max_output_bytes,
    })
}
impl StructuredSvgDocument {
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }
    pub fn to_html(&self) -> Result<String, GxwError> {
        let elements: Vec<_> = self.elements.iter().map(|e| {
            let bytes=self.program.project.source_bytes(&e.source).unwrap_or_default();
            serde_json::json!({"element":e,"bytes_preview":crate::project::spaced_hex(&bytes[..bytes.len().min(64)]),"preview_truncated":bytes.len()>64})
        }).collect();
        let data=serde_json::json!({"sha256":self.source_sha256,"elements":elements,"diagnostics":self.program.diagnostics}).to_string().replace('<',"\\u003c").replace('&',"\\u0026");
        let (a, rest) = include_str!("viewer.html").split_once("{{SVG}}").unwrap();
        let (b, c) = rest.split_once("{{DATA}}").unwrap();
        let html = format!("{a}{}{b}{data}{c}", self.svg);
        check(
            "structured HTML bytes",
            html.len() as u64,
            self.max_output_bytes as u64,
        )?;
        Ok(html)
    }
}
