//! Sony headphones protocol constants and enums
//! Ported from the original C++ implementation

use serde::{Deserialize, Serialize};

// Protocol framing constants
pub const MAX_BLUETOOTH_MESSAGE_SIZE: usize = 2048;
pub const START_MARKER: u8 = 62; // '>'
pub const END_MARKER: u8 = 60;   // '<'

// Escape sequence constants
pub const ESCAPED_BYTE_SENTRY: u8 = 61;
pub const ESCAPED_60: u8 = 44;
pub const ESCAPED_61: u8 = 45;
pub const ESCAPED_62: u8 = 46;

// Device-specific constants
pub const MAX_STEPS_WH_1000_XM3: u8 = 19;
pub const MINIMUM_VOICE_FOCUS_STEP: u8 = 2;

// Service UUID for Sony headphones
pub const SERVICE_UUID: &str = "96CC203E-5068-46ad-B32D-E316F5E069BA";
pub const SERVICE_UUID_BYTES: [u8; 16] = [
    0x96, 0xcc, 0x20, 0x3e, 0x50, 0x68, 0x46, 0xad,
    0xb3, 0x2d, 0xe3, 0x16, 0xf5, 0xe0, 0x69, 0xba,
];

/// Data types for Bluetooth messages
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i8)]
pub enum DataType {
    Data = 0,
    Ack = 1,
    DataMcNo1 = 2,
    DataIcd = 9,
    DataEv = 10,
    DataMdr = 12,
    DataCommon = 13,
    DataMdrNo2 = 14,
    Shot = 16,
    ShotMcNo1 = 18,
    ShotIcd = 25,
    ShotEv = 26,
    ShotMdr = 28,
    ShotCommon = 29,
    ShotMdrNo2 = 30,
    LargeDataCommon = 45,
    Unknown = -1,
}

impl From<u8> for DataType {
    fn from(value: u8) -> Self {
        match value as i8 {
            0 => DataType::Data,
            1 => DataType::Ack,
            2 => DataType::DataMcNo1,
            9 => DataType::DataIcd,
            10 => DataType::DataEv,
            12 => DataType::DataMdr,
            13 => DataType::DataCommon,
            14 => DataType::DataMdrNo2,
            16 => DataType::Shot,
            18 => DataType::ShotMcNo1,
            25 => DataType::ShotIcd,
            26 => DataType::ShotEv,
            28 => DataType::ShotMdr,
            29 => DataType::ShotCommon,
            30 => DataType::ShotMdrNo2,
            45 => DataType::LargeDataCommon,
            _ => DataType::Unknown,
        }
    }
}

/// Command types (PayloadType in Gadgetbridge terminology)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum CommandType {
    // Sound/audio commands (LEA = LE Audio - XM3/XM4 specific, may not work on XM5/XM6)
    VptSetParam = 72,           // 0x48 - LEA_SET_PARAM (VPT/Sound Position - XM3/XM4 only)
    NcAsmSetParam = 104,        // 0x68 - NC/ASM set
    NcAsmGetParam = 102,        // 0x66 - NC/ASM get/inquiry
    NcAsmNotify = 103,          // 0x67 - NC/ASM status notification

    // EQ commands (EQEBB in protocol spec)
    EqGetCapability = 82,       // 0x52 - EQEBB_GET_STATUS
    EqGetParam = 86,            // 0x56 - EQEBB_GET_PARAM (inquiry)
    EqSetParam = 88,            // 0x58 - EQEBB_SET_PARAM (set)
    EqNotify = 89,              // 0x59 - EQEBB_NTFY_PARAM

    // Battery commands
    BatteryGetCapability = 16,  // 0x10 - Get battery capability
    BatteryGetLevel = 18,       // 0x12 - Get battery level
    BatteryNotify = 19,         // 0x13 - Battery level notification

    // Speak-to-chat / System commands (XM5+)
    SpeakToChatGetParam = 246,  // 0xF6 - SYSTEM_GET_PARAM
    SpeakToChatSetParam = 248,  // 0xF8 - SYSTEM_SET_PARAM
    SpeakToChatNotify = 249,    // 0xF9 - SYSTEM_NTFY_PARAM

    // DSEE (audio upsampling)
    DseeGetParam = 230,         // 0xE6 - Get DSEE setting
    DseeSetParam = 232,         // 0xE8 - Set DSEE setting
    DseeNotify = 231,           // 0xE7 - DSEE notification

    // Device info
    DeviceInfoInquiry = 0,      // 0x00 - Device info inquiry
}

/// NC/ASM inquired type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u8)]
pub enum NcAsmInquiredType {
    NoUse = 0,
    NoiseCancelling = 1,
    NoiseCancellingAndAmbientSoundMode = 2,
    AmbientSoundMode = 3,
    // V3 protocol (XM5) uses 0x17
    NoiseCancellingAndAmbientSoundModeV3 = 0x17,
    // XM6 uses 0x19 (25 decimal) - discovered from traffic capture
    NoiseCancellingAndAmbientSoundModeXm6 = 0x19,
}

/// NC/ASM effect (on/off state)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i8)]
pub enum NcAsmEffect {
    Off = 0,
    On = 1,
    AdjustmentInProgress = 16,
    AdjustmentCompletion = 17,
}

/// NC/ASM setting type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i8)]
pub enum NcAsmSettingType {
    OnOff = 0,
    LevelAdjustment = 1,
    DualSingleOff = 2,
}

/// ASM setting type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i8)]
pub enum AsmSettingType {
    OnOff = 0,
    LevelAdjustment = 1,
}

/// ASM ID (normal or voice mode)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i8)]
pub enum AsmId {
    Normal = 0,
    Voice = 1,
}

/// NC dual/single value
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i8)]
pub enum NcDualSingleValue {
    Off = 0,
    Single = 1,
    Dual = 2,
}

/// VPT preset IDs
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i8)]
pub enum VptPresetId {
    Off = 0,
    OutdoorFestival = 1,
    Arena = 2,
    ConcertHall = 3,
    Club = 4,
}

/// Sound position presets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i8)]
pub enum SoundPositionPreset {
    Off = 0,
    FrontLeft = 1,
    FrontRight = 2,
    Front = 3,
    RearLeft = 17,
    RearRight = 18,
    OutOfRange = -1,
}

impl SoundPositionPreset {
    /// Convert from UI index to sound position preset
    pub fn from_index(index: usize) -> Self {
        match index {
            0 => SoundPositionPreset::Off,
            1 => SoundPositionPreset::FrontLeft,
            2 => SoundPositionPreset::FrontRight,
            3 => SoundPositionPreset::Front,
            4 => SoundPositionPreset::RearLeft,
            5 => SoundPositionPreset::RearRight,
            _ => SoundPositionPreset::OutOfRange,
        }
    }

    /// Get the index for UI display
    pub fn to_index(self) -> usize {
        match self {
            SoundPositionPreset::Off => 0,
            SoundPositionPreset::FrontLeft => 1,
            SoundPositionPreset::FrontRight => 2,
            SoundPositionPreset::Front => 3,
            SoundPositionPreset::RearLeft => 4,
            SoundPositionPreset::RearRight => 5,
            SoundPositionPreset::OutOfRange => 6,
        }
    }
}

/// VPT inquired type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[repr(i8)]
pub enum VptInquiredType {
    NoUse = 0,
    Vpt = 1,
    SoundPosition = 2,
    OutOfRange = -1,
}

/// ANC mode for simplified UI control
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AncMode {
    Off,
    NoiseCancelling,
    AmbientSound { level: u8, focus_on_voice: bool },
}

impl Default for AncMode {
    fn default() -> Self {
        AncMode::Off
    }
}
