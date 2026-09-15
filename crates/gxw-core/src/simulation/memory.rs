use super::{
    SimulationProfile,
    compile::{MEMORY_SIZE, X_COUNT, Y_COUNT},
    error,
};
use crate::{DeviceKind, DeviceRef, GxwError};
use std::collections::BTreeSet;

#[derive(Debug, Clone)]
pub(crate) struct Memory {
    pub image: Box<[bool]>,
    pub pending_inputs: [bool; X_COUNT],
    pub outputs: [bool; Y_COUNT],
}
impl Default for Memory {
    fn default() -> Self {
        Self {
            image: vec![false; MEMORY_SIZE].into_boxed_slice(),
            pending_inputs: [false; X_COUNT],
            outputs: [false; Y_COUNT],
        }
    }
}
impl Memory {
    pub fn latch_inputs(&mut self) {
        self.image[..X_COUNT].copy_from_slice(&self.pending_inputs);
    }
    pub fn commit_outputs(&mut self) {
        self.outputs
            .copy_from_slice(&self.image[X_COUNT..X_COUNT + Y_COUNT]);
    }
}
/// Validate a complete update before changing any memory; reject aliased keys.
pub(crate) fn patch(
    profile: SimulationProfile,
    values: &[(String, bool)],
    inputs: bool,
) -> Result<Vec<(DeviceRef, usize, bool)>, GxwError> {
    let mut seen = BTreeSet::new();
    let mut result = Vec::new();
    for (name, value) in values {
        let d = profile.parse_device(name)?;
        if (inputs && d.device != DeviceKind::X)
            || (!inputs && !matches!(d.device, DeviceKind::Y | DeviceKind::M))
        {
            return Err(error(
                "GXW_SIM_UPDATE_TARGET",
                if inputs {
                    "set_inputs accepts only X devices"
                } else {
                    "initial_state/set_devices accept only Y and M devices"
                },
                None,
            ));
        }
        if !seen.insert(d.clone()) {
            return Err(error(
                "GXW_SIM_DUPLICATE_DEVICE",
                format!("Duplicate normalized device: {}", d.name()),
                None,
            ));
        }
        let slot = profile.slot(&d)?;
        result.push((d, slot, *value));
    }
    Ok(result)
}
