//! Isolated scans with a bounded, explicitly selected CPU/timing profile.
mod compile;
mod counters;
mod memory;
mod profiles;
mod scan;
mod timers;
mod trace;

pub use compile::{CompiledInstruction, CompiledProgram, SimulationProfile, compile_program};
pub use counters::CounterState;
pub use scan::{SimulationLimits, Simulator};
pub use timers::TimerState;
pub use trace::{InstructionEvent, ScanSample, ScanSnapshot, ScanTrace};

use crate::GxwError;
fn error(code: &str, message: impl Into<String>, instruction: Option<usize>) -> GxwError {
    GxwError::Simulation {
        code: code.into(),
        message: message.into(),
        instruction,
    }
}
