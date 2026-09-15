use gxw_core::{DeviceProfile, ReadLimits, decode_program, load_csv, load_path};
use std::{path::Path, sync::Arc};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().collect();
    if args.len() != 3 {
        return Err("usage: instructions gxw|csv FILE (FX device syntax)".into());
    }
    let path = Path::new(&args[2]);
    if args[1] == "csv" {
        println!(
            "{}",
            load_csv(path, DeviceProfile::Fx, &ReadLimits::default())?.to_json()?
        );
    } else if args[1] == "gxw" {
        let project = Arc::new(load_path(path, &ReadLimits::default())?);
        let programs = (0..project.programs.len())
            .map(|i| decode_program(project.clone(), i, DeviceProfile::Fx))
            .collect::<Result<Vec<_>, _>>()?;
        println!("{}", serde_json::to_string_pretty(&programs)?);
    } else {
        return Err("expected gxw or csv".into());
    }
    Ok(())
}
