//! Generate our synthetic container fixture; never writes to an existing file.
#[path = "../tests/support/mod.rs"]
mod support;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    use std::io::Write;
    let path = std::env::args_os()
        .nth(1)
        .ok_or("usage: make_fixture NEW_PATH")?;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)?;
    let raw = std::env::args().nth(2).as_deref() == Some("--raw");
    file.write_all(&if raw {
        support::raw_fixture()
    } else {
        support::fixture()
    })?;
    Ok(())
}
