use super::profiles::fx3g;
use serde::Serialize;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CounterState {
    pub device: String,
    pub retentive: bool,
    pub preset: Option<u16>,
    pub value: u16,
    pub input: bool,
    pub previous_input: bool,
    pub rising_edge: bool,
    pub done: bool,
    pub reset_active: bool,
}
impl CounterState {
    pub(crate) fn new(address: u32, preset: Option<u16>) -> Self {
        Self {
            device: format!("C{address}"),
            retentive: fx3g::counter_retentive(address),
            preset,
            value: 0,
            input: false,
            previous_input: false,
            rising_edge: false,
            done: false,
            reset_active: false,
        }
    }
    pub(crate) fn drive(&mut self, input: bool) {
        self.previous_input = self.input;
        self.rising_edge = input && !self.input;
        self.input = input;
        if self.rising_edge && !self.reset_active && !self.done {
            let preset = self
                .preset
                .expect("counter OUT has a constant preset")
                .max(1);
            self.value += 1;
            self.done = self.value >= preset;
        }
    }
    pub(crate) fn reset_coil(&mut self, active: bool) {
        self.reset_active = active;
        if active {
            self.value = 0;
            self.done = false;
            // A reset does not fabricate a new input edge while the input is ON.
        }
    }
    pub(crate) fn stop(&mut self) {
        if !self.retentive {
            let address = self.device[1..].parse().expect("counter address");
            *self = Self::new(address, self.preset);
        }
        self.rising_edge = false;
    }
}
