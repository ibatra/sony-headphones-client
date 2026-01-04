//! Sony headphones Bluetooth protocol implementation
//!
//! This module provides the protocol layer for communicating with Sony headphones
//! via Bluetooth RFCOMM. It includes:
//!
//! - `constants`: Protocol constants, enums, and type definitions
//! - `codec`: Message framing, escaping, and checksum handling
//! - `commands`: Command serialization for headphone features
//! - `models`: Device-specific models and capabilities

pub mod codec;
pub mod commands;
pub mod constants;
pub mod models;

// Re-export commonly used items
pub use codec::{
    extract_message, package_for_bluetooth, unpack_bluetooth_message, CodecError, Message,
};
pub use commands::{
    build_ambient_sound, build_anc_command, build_anc_command_v3, build_anc_command_xm6,
    build_battery_capability_inquiry, build_battery_inquiry, build_dsee_get, build_dsee_set,
    build_eq_capability_inquiry, build_eq_custom, build_eq_get, build_eq_preset, build_nc_asm_inquiry,
    build_noise_cancelling_off, build_noise_cancelling_on, build_sound_position,
    build_speak_to_chat_get, build_speak_to_chat_set, build_vpt_preset,
};
pub use constants::{
    AncMode, AsmId, AsmSettingType, CommandType, DataType, NcAsmEffect, NcAsmInquiredType,
    NcAsmSettingType, NcDualSingleValue, SoundPositionPreset, VptInquiredType, VptPresetId,
    MAX_BLUETOOTH_MESSAGE_SIZE, MAX_STEPS_WH_1000_XM3, SERVICE_UUID, SERVICE_UUID_BYTES,
};
pub use models::{
    BatteryStatus, CustomEq, DseeType, EqPreset, HeadphoneModel, ModelCapabilities,
    ProtocolVersion, SpeakToChat, SpeakToChatSensitivity,
};
