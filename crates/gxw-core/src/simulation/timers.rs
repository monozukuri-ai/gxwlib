use super::profiles::fx3g;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimerState {
    pub device: String,
    pub time_base_ns: u64,
    pub evaluation: &'static str,
    pub retentive: bool,
    pub preset: Option<u16>,
    pub value: u16,
    /// Quantized current value, not wall-clock time since the first ON.
    pub elapsed_ns: u64,
    pub phase_ns: u64,
    pub input: bool,
    pub previous_input: bool,
    pub done: bool,
    pub reset_active: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Timer {
    pub state: TimerState,
    last_evaluation_ns: u64,
    counting: bool,
}
impl Timer {
    pub fn new(address: u32, preset: Option<u16>) -> Self {
        let (time_base_ns, retentive) = fx3g::timer(address).expect("validated timer");
        Self {
            state: TimerState {
                device: format!("T{address}"),
                time_base_ns,
                evaluation: "coil",
                retentive,
                preset,
                value: 0,
                elapsed_ns: 0,
                phase_ns: 0,
                input: false,
                previous_input: false,
                done: false,
                reset_active: false,
            },
            last_evaluation_ns: 0,
            counting: false,
        }
    }
    pub fn snapshot(&self, now: u64, phase: u64) -> TimerState {
        let mut state = self.state.clone();
        state.phase_ns = fx3g::phase(now, phase, state.time_base_ns);
        state
    }
    pub fn drive(&mut self, input: bool, now: u64, phase: u64) {
        let s = &mut self.state;
        let old = s.input;
        s.previous_input = old;
        // Integrate only intervals during which the preceding coil was enabled.
        // The first ON never receives time from before that ON instruction.
        if self.counting && !s.reset_active && !s.done {
            let ticks = fx3g::ticks(now, phase, s.time_base_ns)
                - fx3g::ticks(self.last_evaluation_ns, phase, s.time_base_ns);
            let preset = s.preset.expect("timer OUT has a constant preset");
            s.value = (u64::from(s.value) + ticks).min(u64::from(preset)) as u16;
            // Includes K0: completion requires a subsequent coil evaluation.
            s.done = s.value >= preset;
        }
        s.input = input;
        self.counting = input && !s.reset_active;
        self.last_evaluation_ns = now;
        if s.reset_active || (!input && !s.retentive) {
            s.value = 0;
            s.done = false;
        }
        s.elapsed_ns = u64::from(s.value) * s.time_base_ns;
    }
    pub fn reset_coil(&mut self, active: bool, now: u64) {
        if active || self.state.reset_active {
            self.last_evaluation_ns = now;
        }
        self.state.reset_active = active;
        if active {
            self.counting = false;
            self.state.value = 0;
            self.state.elapsed_ns = 0;
            self.state.done = false;
            self.last_evaluation_ns = now;
        }
    }
    pub fn stop(&mut self, now: u64) {
        if !self.state.retentive {
            let address = self.state.device[1..].parse().expect("timer address");
            *self = Self::new(address, self.state.preset);
        }
        self.rebase(now);
    }
    /// Stopped time never accrues, while oscillator phase remains global.
    pub fn rebase(&mut self, now: u64) {
        self.last_evaluation_ns = now;
    }
}
