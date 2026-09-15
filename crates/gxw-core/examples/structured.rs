use gxw_core::{ReadLimits, load_path, structured::*};
use std::{path::Path, sync::Arc};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 4 {
        return Err("usage: structured pou|declarations FILE INDEX".into());
    }
    let project = Arc::new(load_path(Path::new(&args[2]), &ReadLimits::default())?);
    let index = args[3].parse()?;
    let json = match args[1].as_str() {
        "pou" => decode_structured(project, index, &StructuredLimits::default())?.to_json()?,
        "declarations" => {
            decode_declarations(project, index, &StructuredLimits::default())?.to_json()?
        }
        _ => return Err("unknown mode".into()),
    };
    println!("{json}");
    Ok(())
}
