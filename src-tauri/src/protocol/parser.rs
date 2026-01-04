//! Response parser for Sony headphones protocol
//!
//! Parses device responses (battery notifications, status updates, etc.)
//! that were previously discarded by the ACK handler.

use super::models::BatteryStatus;

/// Command type identifiers for response parsing
/// Reference: ProtocolV2T1.hpp Command enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum ResponseCommand {
    // Power/Battery commands
    PowerRetParam = 0x27,
    PowerNtfyParam = 0x29,

    // NC/ASM commands
    NcAsmRetParam = 0x67,
    NcAsmNtfyParam = 0x69,

    // EQ commands
    EqRetParam = 0x57,
    EqNtfyParam = 0x59,

    // DSEE commands
    UpscalingRetParam = 0xE7,
    UpscalingNtfyParam = 0xE9,
}

/// Power/Battery inquired type
/// Reference: ProtocolV2T1.hpp PowerInquiredType enum
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum PowerInquiredType {
    Battery = 0x00,
    LeftRightBattery = 0x01,
    CradleBattery = 0x02,
    PowerOff = 0x03,
    AutoPowerOff = 0x04,
    AutoPowerOffWearingDetection = 0x05,
    PowerSaveMode = 0x06,
    LinkControl = 0x07,
    BatteryWithThreshold = 0x08,
    LeftRightBatteryWithThreshold = 0x09,
    CradleBatteryWithThreshold = 0x0A,
    BatterySafeMode = 0x0B,
}

/// Parsed response from device
#[derive(Debug, Clone)]
pub enum ParsedResponse {
    /// Battery status update
    Battery(BatteryStatus),
    /// ANC/Ambient mode status
    AncStatus { mode: u8, level: u8 },
    /// EQ status
    EqStatus { preset: u8 },
    /// DSEE status
    DseeStatus { enabled: bool },
    /// Unknown/unparsed response
    Unknown { command: u8, data: Vec<u8> },
}

/// Parse a device response payload
///
/// The payload format (after message framing is removed):
/// [data_type, seq, size[4], command, inquired_type, ...data..., checksum]
///
/// We receive the buffer after START/END markers are stripped, so format is:
/// [data_type, seq, size[4], payload..., checksum]
///
/// The payload itself contains: [command, inquired_type, ...data...]
pub fn parse_response(payload: &[u8]) -> Option<ParsedResponse> {
    // Minimum valid response: data_type(1) + seq(1) + size(4) + command(1) + type(1) + checksum(1)
    if payload.len() < 9 {
        tracing::debug!("Response too short: {} bytes", payload.len());
        return None;
    }

    // Skip header: data_type(1) + seq(1) + size(4) = 6 bytes
    // The actual command data starts at offset 6
    let data = &payload[6..payload.len() - 1]; // Exclude final checksum

    if data.is_empty() {
        return None;
    }

    let command = data[0];

    tracing::info!(
        "📦 Parsing response: command=0x{:02X}, {} bytes data",
        command, data.len()
    );

    match command {
        0x27 | 0x29 => parse_battery_response(data), // POWER_RET_PARAM / POWER_NTFY_PARAM
        0x67 | 0x69 => parse_nc_asm_response(data),  // NC_ASM_RET_PARAM / NC_ASM_NTFY_PARAM
        0x57 | 0x59 => parse_eq_response(data),      // EQ_RET_PARAM / EQ_NTFY_PARAM
        0xE7 | 0xE9 => parse_dsee_response(data),    // UPSCALING_RET/NTFY_PARAM
        _ => {
            tracing::debug!("Unknown command type: 0x{:02X}", command);
            Some(ParsedResponse::Unknown {
                command,
                data: data.to_vec(),
            })
        }
    }
}

/// Parse battery response
/// Format: [command, inquired_type, battery_data...]
fn parse_battery_response(data: &[u8]) -> Option<ParsedResponse> {
    if data.len() < 3 {
        return None;
    }

    let inquired_type = data[1];
    let battery_data = &data[2..];

    tracing::info!(
        "🔋 Parsing battery response: type=0x{:02X}, {} bytes",
        inquired_type, battery_data.len()
    );

    match inquired_type {
        // Single battery (over-ear headphones)
        0x00 | 0x08 => {
            // BATTERY or BATTERY_WITH_THRESHOLD
            if battery_data.len() >= 2 {
                let level = battery_data[0];
                let charging = battery_data[1] != 0;
                tracing::info!("🔋 Single battery: {}% charging={}", level, charging);
                Some(ParsedResponse::Battery(BatteryStatus::single(level, charging)))
            } else {
                None
            }
        }
        // Left/Right battery (earbuds)
        0x01 | 0x09 => {
            // LEFT_RIGHT_BATTERY or LR_BATTERY_WITH_THRESHOLD
            if battery_data.len() >= 4 {
                let left_level = battery_data[0];
                let left_charging = battery_data[1] != 0;
                let right_level = battery_data[2];
                let right_charging = battery_data[3] != 0;
                tracing::info!(
                    "🔋 L/R battery: L={}% R={}% charging L={} R={}",
                    left_level, right_level, left_charging, right_charging
                );
                Some(ParsedResponse::Battery(BatteryStatus::dual(
                    left_level, left_charging,
                    right_level, right_charging,
                )))
            } else {
                None
            }
        }
        // Cradle/Case battery
        0x02 | 0x0A => {
            // CRADLE_BATTERY or CRADLE_BATTERY_WITH_THRESHOLD
            if battery_data.len() >= 2 {
                let level = battery_data[0];
                let charging = battery_data[1] != 0;
                tracing::info!("🔋 Case battery: {}% charging={}", level, charging);
                // Return as case-only battery (need to combine with L/R in state)
                Some(ParsedResponse::Battery(
                    BatteryStatus::single(0, false).with_case(level, charging)
                ))
            } else {
                None
            }
        }
        _ => {
            tracing::debug!("Unknown battery type: 0x{:02X}", inquired_type);
            None
        }
    }
}

/// Parse NC/ASM response
fn parse_nc_asm_response(data: &[u8]) -> Option<ParsedResponse> {
    if data.len() < 4 {
        return None;
    }

    // Format: [command, inquired_type, mode, level, ...]
    let mode = data[2];
    let level = if data.len() > 3 { data[3] } else { 0 };

    tracing::info!("🎧 NC/ASM status: mode={} level={}", mode, level);

    Some(ParsedResponse::AncStatus { mode, level })
}

/// Parse EQ response
fn parse_eq_response(data: &[u8]) -> Option<ParsedResponse> {
    if data.len() < 3 {
        return None;
    }

    // Format: [command, inquired_type, preset, ...]
    let preset = data[2];

    tracing::info!("🎵 EQ status: preset=0x{:02X}", preset);

    Some(ParsedResponse::EqStatus { preset })
}

/// Parse DSEE response
fn parse_dsee_response(data: &[u8]) -> Option<ParsedResponse> {
    if data.len() < 3 {
        return None;
    }

    // Format: [command, inquired_type, enabled, ...]
    let enabled = data[2] != 0;

    tracing::info!("🔊 DSEE status: enabled={}", enabled);

    Some(ParsedResponse::DseeStatus { enabled })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_single_battery() {
        // Simulated response: data_type, seq, size[4], command=0x27, type=0x00, level=80, charging=0, checksum
        let payload = vec![0x0C, 0x00, 0x00, 0x00, 0x00, 0x04, 0x27, 0x00, 80, 0, 0x00];

        if let Some(ParsedResponse::Battery(status)) = parse_response(&payload) {
            assert_eq!(status.level, 80);
            assert!(!status.charging);
        } else {
            panic!("Expected battery response");
        }
    }

    #[test]
    fn test_parse_dual_battery() {
        // Simulated response for earbuds
        let payload = vec![0x0C, 0x00, 0x00, 0x00, 0x00, 0x06, 0x27, 0x01, 75, 0, 80, 1, 0x00];

        if let Some(ParsedResponse::Battery(status)) = parse_response(&payload) {
            assert_eq!(status.level, 75); // Left
            assert!(!status.charging);
            assert_eq!(status.right_level, Some(80));
            assert_eq!(status.right_charging, Some(true));
        } else {
            panic!("Expected battery response");
        }
    }
}
