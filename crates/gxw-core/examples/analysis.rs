//! Independent Rust entry point for binding/artifact parity checks (FX syntax).
use gxw_core::*;
use std::{path::Path, sync::Arc};
fn programs(
    format: &str,
    path: &str,
) -> Result<Vec<Arc<InstructionProgram>>, Box<dyn std::error::Error>> {
    if format == "csv" {
        Ok(vec![Arc::new(load_csv(
            Path::new(path),
            DeviceProfile::Fx,
            &ReadLimits::default(),
        )?)])
    } else if format == "gxw" {
        let raw = Arc::new(load_path(Path::new(path), &ReadLimits::default())?);
        (0..raw.programs.len())
            .map(|i| {
                decode_program(raw.clone(), i, DeviceProfile::Fx)
                    .map(Arc::new)
                    .map_err(Into::into)
            })
            .collect()
    } else {
        Err("expected gxw or csv".into())
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 && args.len() != 5 {
        return Err("usage: analysis gxw|csv FILE [gxw|csv OTHER_FILE]".into());
    }
    let left = programs(&args[1], &args[2])?;
    if args.len() == 5 {
        let right = programs(&args[3], &args[4])?;
        let (Some(a), Some(b)) = (left.first(), right.first()) else {
            return Err("diff requires a program on both sides".into());
        };
        println!(
            "{}",
            diff_programs(a.clone(), b.clone(), 1_000_000)?.to_json()?
        );
    } else {
        let mut output = Vec::new();
        for program in left {
            let report = analyze(program.clone(), &AnalysisOptions::default())?;
            let graph = Arc::new(build_ladder(program.clone(), &AnalysisOptions::default())?);
            let document = render_svg(graph.clone(), &RenderOptions::default())?;
            output.push(serde_json::json!({"analysis":report,"graph":&*graph,"document":document,"svg":document.svg,"html":document.to_html()?,"diff_self":diff_programs(program.clone(),program,1_000_000)?}));
        }
        println!("{}", serde_json::to_string(&output)?);
    }
    Ok(())
}
