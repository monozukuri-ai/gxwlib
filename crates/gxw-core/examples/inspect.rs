use std::{
    io::{self, Write},
    path::Path,
};

fn main() {
    if let Err(error) = run() {
        eprintln!("gxw inspect: {error}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let mut args = std::env::args_os().skip(1);
    let path = args.next().ok_or("usage: inspect FILE.gxw")?;
    if args.next().is_some() {
        return Err("usage: inspect FILE.gxw".into());
    }
    let index = gxw_core::inspect_path(Path::new(&path), &gxw_core::ReadLimits::default())?;
    writeln!(io::stdout().lock(), "{}", index.to_json()?)?;
    Ok(())
}
