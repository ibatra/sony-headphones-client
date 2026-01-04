//! Device-specific protocol models for Sony headphones
//!
//! Different headphone generations have different capabilities and protocol variations.
//! This module provides abstractions to handle these differences.

use serde::{Deserialize, Serialize};

/// Supported Sony headphone models
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HeadphoneModel {
    Xm3,  // WH-1000XM3
    Xm4,  // WH-1000XM4
    Xm5,  // WH-1000XM5
    Xm6,  // WH-1000XM6
    WfXm3, // WF-1000XM3 earbuds
    WfXm4, // WF-1000XM4 earbuds
    WfXm5, // WF-1000XM5 earbuds
    Unknown,
}

impl HeadphoneModel {
    /// Detect model from Bluetooth device name
    pub fn from_device_name(name: &str) -> Self {
        let name_lower = name.to_lowercase();

        // Check earbuds (WF series) FIRST since "wf-1000xm4" contains "xm4"
        if name_lower.contains("wf-1000xm5") {
            HeadphoneModel::WfXm5
        } else if name_lower.contains("wf-1000xm4") {
            HeadphoneModel::WfXm4
        } else if name_lower.contains("wf-1000xm3") {
            HeadphoneModel::WfXm3
        }
        // Over-ear headphones (WH series)
        else if name_lower.contains("wh-1000xm6") || name_lower.contains("xm6") {
            HeadphoneModel::Xm6
        } else if name_lower.contains("wh-1000xm5") || name_lower.contains("xm5") {
            HeadphoneModel::Xm5
        } else if name_lower.contains("wh-1000xm4") || name_lower.contains("xm4") {
            HeadphoneModel::Xm4
        } else if name_lower.contains("wh-1000xm3") || name_lower.contains("xm3") {
            HeadphoneModel::Xm3
        } else {
            HeadphoneModel::Unknown
        }
    }

    /// Display name for the model
    pub fn display_name(&self) -> &'static str {
        match self {
            HeadphoneModel::Xm3 => "WH-1000XM3",
            HeadphoneModel::Xm4 => "WH-1000XM4",
            HeadphoneModel::Xm5 => "WH-1000XM5",
            HeadphoneModel::Xm6 => "WH-1000XM6",
            HeadphoneModel::WfXm3 => "WF-1000XM3",
            HeadphoneModel::WfXm4 => "WF-1000XM4",
            HeadphoneModel::WfXm5 => "WF-1000XM5",
            HeadphoneModel::Unknown => "Unknown Sony Headphones",
        }
    }

    /// Protocol version used by this model
    pub fn protocol_version(&self) -> ProtocolVersion {
        match self {
            HeadphoneModel::Xm3 => ProtocolVersion::V1,
            HeadphoneModel::Xm4 => ProtocolVersion::V1,
            HeadphoneModel::WfXm3 => ProtocolVersion::V1,
            HeadphoneModel::WfXm4 => ProtocolVersion::V2,
            HeadphoneModel::Xm5 => ProtocolVersion::V3,
            HeadphoneModel::Xm6 => ProtocolVersion::V3,
            HeadphoneModel::WfXm5 => ProtocolVersion::V3,
            HeadphoneModel::Unknown => ProtocolVersion::V1, // Fall back to basic protocol
        }
    }
}

/// Protocol version for different generations
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProtocolVersion {
    V1, // XM3, XM4, WF-XM3
    V2, // WF-XM4
    V3, // XM5, XM6, WF-XM5
}

/// Capabilities supported by a headphone model
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelCapabilities {
    /// Model identifier
    pub model: HeadphoneModel,

    /// Maximum ambient sound level (0-20 typically)
    pub max_ambient_level: u8,

    /// Supports focus on voice in ambient mode
    pub supports_focus_on_voice: bool,

    /// Supports speak-to-chat auto-pause
    pub supports_speak_to_chat: bool,

    /// Supports adaptive noise cancelling
    pub supports_adaptive_nc: bool,

    /// Supports wind noise reduction mode
    pub supports_wind_noise_reduction: bool,

    /// Supports equalizer
    pub supports_equalizer: bool,

    /// Number of EQ bands (5 or 10)
    pub eq_bands: u8,

    /// Supports DSEE audio upsampling
    pub supports_dsee: bool,

    /// DSEE type (HX for older, Extreme for newer)
    pub dsee_type: DseeType,

    /// Has dual batteries (earbuds) vs single (over-ear)
    pub dual_battery: bool,

    /// Supports case battery (earbuds only)
    pub case_battery: bool,

    /// Supports multipoint Bluetooth
    pub supports_multipoint: bool,

    /// Supports VPT surround sound
    pub supports_vpt: bool,

    /// Supports sound position
    pub supports_sound_position: bool,
}

/// DSEE audio upsampling type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DseeType {
    None,
    Hx,      // DSEE HX (older models)
    Extreme, // DSEE Extreme (newer models)
}

impl ModelCapabilities {
    /// Get capabilities for a specific model
    pub fn for_model(model: HeadphoneModel) -> Self {
        match model {
            HeadphoneModel::Xm3 => Self::xm3(),
            HeadphoneModel::Xm4 => Self::xm4(),
            HeadphoneModel::Xm5 => Self::xm5(),
            HeadphoneModel::Xm6 => Self::xm6(),
            HeadphoneModel::WfXm3 => Self::wf_xm3(),
            HeadphoneModel::WfXm4 => Self::wf_xm4(),
            HeadphoneModel::WfXm5 => Self::wf_xm5(),
            HeadphoneModel::Unknown => Self::xm4(), // Default to XM4 capabilities
        }
    }

    fn xm3() -> Self {
        Self {
            model: HeadphoneModel::Xm3,
            max_ambient_level: 19,
            supports_focus_on_voice: true,
            supports_speak_to_chat: false,
            supports_adaptive_nc: false,
            supports_wind_noise_reduction: false,
            supports_equalizer: true,
            eq_bands: 5,
            supports_dsee: true,
            dsee_type: DseeType::Hx,
            dual_battery: false,
            case_battery: false,
            supports_multipoint: false,
            supports_vpt: true,
            supports_sound_position: true,
        }
    }

    fn xm4() -> Self {
        Self {
            model: HeadphoneModel::Xm4,
            max_ambient_level: 20,
            supports_focus_on_voice: true,
            supports_speak_to_chat: true,
            supports_adaptive_nc: false,
            supports_wind_noise_reduction: true,
            supports_equalizer: true,
            eq_bands: 5,
            supports_dsee: true,
            dsee_type: DseeType::Extreme,
            dual_battery: false,
            case_battery: false,
            supports_multipoint: true,
            supports_vpt: true,
            supports_sound_position: true,
        }
    }

    fn xm5() -> Self {
        Self {
            model: HeadphoneModel::Xm5,
            max_ambient_level: 20,
            supports_focus_on_voice: true,
            supports_speak_to_chat: true,
            supports_adaptive_nc: true,
            supports_wind_noise_reduction: true,
            supports_equalizer: true,
            eq_bands: 5,
            supports_dsee: true,
            dsee_type: DseeType::Extreme,
            dual_battery: false,
            case_battery: false,
            supports_multipoint: true,
            supports_vpt: false, // XM5 removed VPT
            supports_sound_position: false,
        }
    }

    fn xm6() -> Self {
        // XM6 likely similar to XM5 with potential enhancements
        Self {
            model: HeadphoneModel::Xm6,
            max_ambient_level: 20,
            supports_focus_on_voice: true,
            supports_speak_to_chat: true,
            supports_adaptive_nc: true,
            supports_wind_noise_reduction: true,
            supports_equalizer: true,
            eq_bands: 5,
            supports_dsee: true,
            dsee_type: DseeType::Extreme,
            dual_battery: false,
            case_battery: false,
            supports_multipoint: true,
            supports_vpt: false, // Likely same as XM5
            supports_sound_position: false,
        }
    }

    fn wf_xm3() -> Self {
        Self {
            model: HeadphoneModel::WfXm3,
            max_ambient_level: 20,
            supports_focus_on_voice: true,
            supports_speak_to_chat: false,
            supports_adaptive_nc: false,
            supports_wind_noise_reduction: false,
            supports_equalizer: true,
            eq_bands: 5,
            supports_dsee: true,
            dsee_type: DseeType::Hx,
            dual_battery: true, // Left + Right earbuds
            case_battery: true,
            supports_multipoint: false,
            supports_vpt: false,
            supports_sound_position: false,
        }
    }

    fn wf_xm4() -> Self {
        Self {
            model: HeadphoneModel::WfXm4,
            max_ambient_level: 20,
            supports_focus_on_voice: true,
            supports_speak_to_chat: true,
            supports_adaptive_nc: false,
            supports_wind_noise_reduction: true,
            supports_equalizer: true,
            eq_bands: 5,
            supports_dsee: true,
            dsee_type: DseeType::Extreme,
            dual_battery: true,
            case_battery: true,
            supports_multipoint: true,
            supports_vpt: false,
            supports_sound_position: false,
        }
    }

    fn wf_xm5() -> Self {
        Self {
            model: HeadphoneModel::WfXm5,
            max_ambient_level: 20,
            supports_focus_on_voice: true,
            supports_speak_to_chat: true,
            supports_adaptive_nc: true,
            supports_wind_noise_reduction: true,
            supports_equalizer: true,
            eq_bands: 5,
            supports_dsee: true,
            dsee_type: DseeType::Extreme,
            dual_battery: true,
            case_battery: true,
            supports_multipoint: true,
            supports_vpt: false,
            supports_sound_position: false,
        }
    }
}

/// Battery status for a device
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryStatus {
    /// Main battery level (0-100), or left earbud for dual battery
    pub level: u8,

    /// Whether main battery is charging
    pub charging: bool,

    /// Right earbud battery for dual battery devices
    pub right_level: Option<u8>,

    /// Whether right earbud is charging
    pub right_charging: Option<bool>,

    /// Case battery level for earbuds
    pub case_level: Option<u8>,

    /// Whether case is charging
    pub case_charging: Option<bool>,
}

impl BatteryStatus {
    /// Create single battery status (over-ear headphones)
    pub fn single(level: u8, charging: bool) -> Self {
        Self {
            level,
            charging,
            right_level: None,
            right_charging: None,
            case_level: None,
            case_charging: None,
        }
    }

    /// Create dual battery status (earbuds)
    pub fn dual(left: u8, left_charging: bool, right: u8, right_charging: bool) -> Self {
        Self {
            level: left,
            charging: left_charging,
            right_level: Some(right),
            right_charging: Some(right_charging),
            case_level: None,
            case_charging: None,
        }
    }

    /// Add case battery info
    pub fn with_case(mut self, level: u8, charging: bool) -> Self {
        self.case_level = Some(level);
        self.case_charging = Some(charging);
        self
    }
}

/// EQ preset identifiers
/// Values from reference: ProtocolV2T1.hpp EqPresetId
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum EqPreset {
    Off = 0x00,
    // Genre presets (0x01-0x07)
    Rock = 0x01,
    Pop = 0x02,
    Jazz = 0x03,
    Dance = 0x04,
    Edm = 0x05,
    RnbHipHop = 0x06,
    Acoustic = 0x07,
    // Sony presets (0x10-0x17)
    Bright = 0x10,
    Excited = 0x11,
    Mellow = 0x12,
    Relaxed = 0x13,
    Vocal = 0x14,
    Treble = 0x15,
    Bass = 0x16,
    Speech = 0x17,
    // Custom presets
    Custom1 = 0xA0,
    Custom2 = 0xA1,
}

/// Custom EQ band settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomEq {
    /// Band values (typically 5 bands, each -10 to +10)
    pub bands: Vec<i8>,

    /// Bass boost level
    pub bass_level: i8,
}

impl Default for CustomEq {
    fn default() -> Self {
        Self {
            bands: vec![0; 5],
            bass_level: 0,
        }
    }
}

/// Speak-to-chat settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakToChat {
    /// Feature enabled
    pub enabled: bool,

    /// Sensitivity (low/medium/high)
    pub sensitivity: SpeakToChatSensitivity,

    /// Auto-close timeout in seconds
    pub timeout_seconds: u8,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpeakToChatSensitivity {
    Low,
    Medium,
    High,
}

impl Default for SpeakToChat {
    fn default() -> Self {
        Self {
            enabled: false,
            sensitivity: SpeakToChatSensitivity::Medium,
            timeout_seconds: 15,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_detection() {
        assert_eq!(
            HeadphoneModel::from_device_name("WH-1000XM6"),
            HeadphoneModel::Xm6
        );
        assert_eq!(
            HeadphoneModel::from_device_name("LE_WH-1000XM5"),
            HeadphoneModel::Xm5
        );
        assert_eq!(
            HeadphoneModel::from_device_name("WF-1000XM4"),
            HeadphoneModel::WfXm4
        );
        assert_eq!(
            HeadphoneModel::from_device_name("XM4"),
            HeadphoneModel::Xm4
        );
    }

    #[test]
    fn test_capabilities() {
        let xm5 = ModelCapabilities::for_model(HeadphoneModel::Xm5);
        assert!(xm5.supports_speak_to_chat);
        assert!(!xm5.supports_vpt); // XM5 removed VPT

        let xm3 = ModelCapabilities::for_model(HeadphoneModel::Xm3);
        assert!(!xm3.supports_speak_to_chat);
        assert!(xm3.supports_vpt);
    }
}
