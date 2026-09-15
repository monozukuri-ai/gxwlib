//! Explicit device spelling profiles for structural inspection, never CPU validation.
//! Q/L X/Y radix: Mitsubishi SH-080785ENG-M (device memory table).
//! CPU model/ranges, instruction support, timers and task order remain unverified.
use crate::{DeviceKind, GxwError};
use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CpuFamily {
    Fx,
    Q,
    L,
}
impl CpuFamily {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Fx => "fx",
            Self::Q => "q",
            Self::L => "l",
        }
    }
}
#[derive(Debug, Clone, Serialize)]
pub struct DeviceAddress {
    pub family: CpuFamily,
    pub device: DeviceKind,
    pub address: u32,
    pub radix: u32,
    pub canonical: String,
    pub range_status: String,
}
/// Recognize only literal X/Y/M/D/T/C addresses. No labels, indices or CPU inference.
pub fn parse_device_address(text: &str, family: CpuFamily) -> Result<DeviceAddress, GxwError> {
    let bytes = text.as_bytes();
    let device = match bytes.first().map(u8::to_ascii_uppercase) {
        Some(b'X') => DeviceKind::X,
        Some(b'Y') => DeviceKind::Y,
        Some(b'M') => DeviceKind::M,
        Some(b'D') => DeviceKind::D,
        Some(b'T') => DeviceKind::T,
        Some(b'C') => DeviceKind::C,
        _ => {
            return Err(super::unsupported(
                "device syntax supports only X/Y/M/D/T/C literals",
            ));
        }
    };
    let radix = if matches!(device, DeviceKind::X | DeviceKind::Y) {
        if family == CpuFamily::Fx { 8 } else { 16 }
    } else {
        10
    };
    let digits = &text[1..];
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii() && c.is_digit(radix)) {
        return Err(GxwError::format(
            "device address",
            "invalid digits for explicit family",
        ));
    }
    let address =
        u32::from_str_radix(digits, radix).map_err(|e| GxwError::format("device address", e))?;
    let value = match radix {
        8 => format!("{address:o}"),
        16 => format!("{address:X}"),
        _ => address.to_string(),
    };
    Ok(DeviceAddress {
        family,
        device,
        address,
        radix,
        canonical: format!("{}{value}", bytes[0].to_ascii_uppercase() as char),
        range_status: "not_checked".into(),
    })
}
