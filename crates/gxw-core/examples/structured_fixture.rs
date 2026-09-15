//! Regenerate the self-authored public fixture, never a GX Works2 writer.
#[path = "../tests/support/structured.rs"]
mod control;
#[path = "../tests/support/mod.rs"]
mod support;
fn main() -> std::io::Result<()> {
    std::fs::write("tests/fixtures/structured.gxw", control::fixture())
}
