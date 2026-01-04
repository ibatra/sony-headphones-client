//! Sony headphones command serialization
//! Builds command payloads for various headphone features

use super::constants::*;
use super::models::{EqPreset, SpeakToChatSensitivity};

/// Get dual/single value based on ASM level
pub fn get_dual_single_for_asm_level(asm_level: u8) -> NcDualSingleValue {
    match asm_level {
        0 => NcDualSingleValue::Dual,
        1 => NcDualSingleValue::Single,
        _ => NcDualSingleValue::Off,
    }
}

/// Serialize NC and ASM settings command
/// This controls noise cancelling and ambient sound mode
pub fn serialize_nc_asm_setting(
    nc_asm_effect: NcAsmEffect,
    nc_asm_setting_type: NcAsmSettingType,
    asm_setting_type: AsmSettingType,
    asm_id: AsmId,
    asm_level: u8,
) -> Vec<u8> {
    vec![
        CommandType::NcAsmSetParam as u8,
        NcAsmInquiredType::NoiseCancellingAndAmbientSoundMode as i8 as u8,
        nc_asm_effect as i8 as u8,
        nc_asm_setting_type as i8 as u8,
        get_dual_single_for_asm_level(asm_level) as i8 as u8,
        asm_setting_type as i8 as u8,
        asm_id as i8 as u8,
        asm_level,
    ]
}

/// Serialize VPT (Virtual Phone Technology) setting command
pub fn serialize_vpt_setting(vpt_type: VptInquiredType, preset: u8) -> Vec<u8> {
    vec![
        CommandType::VptSetParam as u8,
        vpt_type as i8 as u8,
        preset,
    ]
}

/// High-level command builders for common operations

/// Build command to enable noise cancelling (V1/V2 protocol)
pub fn build_noise_cancelling_on() -> Vec<u8> {
    serialize_nc_asm_setting(
        NcAsmEffect::On,
        NcAsmSettingType::OnOff,
        AsmSettingType::OnOff,
        AsmId::Normal,
        0,
    )
}

/// Build command to disable noise cancelling (off) (V1/V2 protocol)
pub fn build_noise_cancelling_off() -> Vec<u8> {
    serialize_nc_asm_setting(
        NcAsmEffect::Off,
        NcAsmSettingType::OnOff,
        AsmSettingType::OnOff,
        AsmId::Normal,
        0,
    )
}

/// Build command to enable ambient sound mode with a specific level (V1/V2 protocol)
pub fn build_ambient_sound(level: u8, focus_on_voice: bool) -> Vec<u8> {
    let asm_id = if focus_on_voice && level >= MINIMUM_VOICE_FOCUS_STEP {
        AsmId::Voice
    } else {
        AsmId::Normal
    };

    serialize_nc_asm_setting(
        NcAsmEffect::On,
        NcAsmSettingType::LevelAdjustment,
        AsmSettingType::LevelAdjustment,
        asm_id,
        level,
    )
}

// ============================================================================
// V3 Protocol Commands (XM5/WF-XM5)
// ============================================================================

/// V3 NC/ASM modes
#[repr(u8)]
pub enum V3NcAsmMode {
    Off = 0x00,
    NoiseCancelling = 0x01,
    AmbientSound = 0x02,
    WindReduction = 0x03,
}

/// Build V3 NC/ASM command (for XM5)
pub fn build_nc_asm_v3(mode: V3NcAsmMode, asm_level: u8, focus_on_voice: bool) -> Vec<u8> {
    vec![
        CommandType::NcAsmSetParam as u8,
        NcAsmInquiredType::NoiseCancellingAndAmbientSoundModeV3 as u8,
        mode as u8,
        0x01, // ASM enabled when in ambient mode
        asm_level,
        if focus_on_voice { 0x01 } else { 0x00 },
    ]
}

/// Build V3 command from ANC mode enum (for XM5)
pub fn build_anc_command_v3(mode: AncMode) -> Vec<u8> {
    match mode {
        AncMode::Off => build_nc_asm_v3(V3NcAsmMode::Off, 0, false),
        AncMode::NoiseCancelling => build_nc_asm_v3(V3NcAsmMode::NoiseCancelling, 0, false),
        AncMode::AmbientSound { level, focus_on_voice } => {
            build_nc_asm_v3(V3NcAsmMode::AmbientSound, level, focus_on_voice)
        }
    }
}

// ============================================================================
// XM6 Protocol Commands (WH-1000XM6)
// Discovered from Bluetooth HCI traffic capture of Sony Headphones Connect app
// ============================================================================

/// Build XM6 NC/ASM command
/// Payload structure from traffic capture (CORRECTED - positions 2 and 3 swapped):
/// [68, 19, 01, enable, asm_mode, nc_mode, asm_level, focus_voice, 00]
/// NC ON:  68:19:01:01:00:01:14:00:00 (enable=01, asm=00, nc=01)
/// ASM ON: 68:19:01:01:01:00:0A:00:00 (enable=01, asm=01, nc=00)
/// OFF:    68:19:01:00:00:00:14:00:00 (enable=00, asm=00, nc=00)
pub fn build_nc_asm_xm6(nc_on: bool, asm_on: bool, asm_level: u8, focus_on_voice: bool) -> Vec<u8> {
    let any_mode_active = nc_on || asm_on;
    vec![
        CommandType::NcAsmSetParam as u8,          // 0x68
        NcAsmInquiredType::NoiseCancellingAndAmbientSoundModeXm6 as u8, // 0x19
        0x01,                                       // Sub-type (always 01)
        if any_mode_active { 0x01 } else { 0x00 }, // Enable (01 when NC OR ASM active)
        if asm_on { 0x01 } else { 0x00 },          // ASM mode on/off (SWAPPED)
        if nc_on { 0x01 } else { 0x00 },           // NC mode on/off (SWAPPED)
        asm_level,                                  // ASM level (0-20)
        if focus_on_voice { 0x01 } else { 0x00 },  // Focus on voice
        0x00,                                       // Reserved
    ]
}

/// Build XM6 command from ANC mode enum
pub fn build_anc_command_xm6(mode: AncMode) -> Vec<u8> {
    match mode {
        AncMode::Off => build_nc_asm_xm6(false, false, 0x14, false),
        AncMode::NoiseCancelling => build_nc_asm_xm6(true, false, 0x14, false),
        AncMode::AmbientSound { level, focus_on_voice } => {
            build_nc_asm_xm6(false, true, level, focus_on_voice)
        }
    }
}

/// Build command from ANC mode enum (V1/V2 protocol)
pub fn build_anc_command(mode: AncMode) -> Vec<u8> {
    match mode {
        AncMode::Off => build_noise_cancelling_off(),
        AncMode::NoiseCancelling => build_noise_cancelling_on(),
        AncMode::AmbientSound { level, focus_on_voice } => {
            build_ambient_sound(level, focus_on_voice)
        }
    }
}

/// Build command to set VPT preset
pub fn build_vpt_preset(preset: VptPresetId) -> Vec<u8> {
    serialize_vpt_setting(VptInquiredType::Vpt, preset as i8 as u8)
}

/// Build command to set sound position
pub fn build_sound_position(position: SoundPositionPreset) -> Vec<u8> {
    serialize_vpt_setting(VptInquiredType::SoundPosition, position as i8 as u8)
}

// ============================================================================
// Battery Commands
// ============================================================================

/// Build command to inquire battery level
pub fn build_battery_inquiry() -> Vec<u8> {
    vec![CommandType::BatteryGetLevel as u8, 0x01]
}

/// Build command to get battery capability (single vs dual battery)
pub fn build_battery_capability_inquiry() -> Vec<u8> {
    vec![CommandType::BatteryGetCapability as u8, 0x01]
}

// ============================================================================
// EQ Commands
// ============================================================================

/// Build command to get EQ capabilities
pub fn build_eq_capability_inquiry() -> Vec<u8> {
    vec![CommandType::EqGetCapability as u8, 0x01]
}

/// Build command to get current EQ settings
pub fn build_eq_get() -> Vec<u8> {
    vec![CommandType::EqGetParam as u8, 0x01]
}

/// Build command to set EQ preset
pub fn build_eq_preset(preset: EqPreset) -> Vec<u8> {
    vec![
        CommandType::EqSetParam as u8,
        0x01, // EQ type indicator
        preset as u8,
    ]
}

/// Build command to set custom EQ bands
/// bands: 5 values from 0-20 (representing -10 to +10)
/// bass_level: 0-20 (representing -10 to +10)
pub fn build_eq_custom(bands: &[u8], bass_level: u8) -> Vec<u8> {
    let mut cmd = vec![
        CommandType::EqSetParam as u8,
        0x01, // EQ type indicator
        EqPreset::Custom1 as u8,
    ];
    // Add band values (typically 5 bands)
    cmd.extend_from_slice(bands);
    // Add bass level
    cmd.push(bass_level);
    cmd
}

// ============================================================================
// Speak-to-Chat Commands (XM5/XM6)
// ============================================================================

/// Build command to get speak-to-chat settings
/// Uses SMART_TALKING_MODE_TYPE1 = 0x02 as inquired type
pub fn build_speak_to_chat_get() -> Vec<u8> {
    vec![CommandType::SpeakToChatGetParam as u8, 0x02]
}

/// Build command to set speak-to-chat
/// enabled: whether the feature is on
/// sensitivity: Low/Medium/High (maps to DetectSensitivity: AUTO=0, HIGH=1, LOW=2)
/// timeout: auto-close timeout (maps to ModeOutTime: FAST=0, MID=1, SLOW=2, NONE=3)
pub fn build_speak_to_chat_set(enabled: bool, sensitivity: SpeakToChatSensitivity, timeout: u8) -> Vec<u8> {
    // Reference: DetectSensitivity enum - AUTO=0, HIGH=1, LOW=2
    let sensitivity_byte = match sensitivity {
        SpeakToChatSensitivity::High => 0x01,   // HIGH
        SpeakToChatSensitivity::Medium => 0x00, // AUTO (medium maps to auto)
        SpeakToChatSensitivity::Low => 0x02,    // LOW
    };

    // Reference: ModeOutTime enum - FAST=0 (~5s), MID=1 (~15s), SLOW=2 (~30s), NONE=3 (don't end)
    let timeout_byte = match timeout {
        0 => 0x03,  // Never (NONE)
        5 => 0x00,  // Fast (~5s)
        10 | 15 => 0x01, // Mid (~15s)
        30 => 0x02, // Slow (~30s)
        _ => 0x01,  // Default to mid
    };

    vec![
        CommandType::SpeakToChatSetParam as u8,
        0x02,                      // SMART_TALKING_MODE_TYPE1 inquired type
        if enabled { 0x01 } else { 0x00 },
        sensitivity_byte,
        timeout_byte,
    ]
}

// ============================================================================
// DSEE Commands (Audio Upsampling)
// ============================================================================

/// Build command to get DSEE setting
pub fn build_dsee_get() -> Vec<u8> {
    vec![CommandType::DseeGetParam as u8, 0x01]
}

/// Build command to set DSEE on/off
pub fn build_dsee_set(enabled: bool) -> Vec<u8> {
    vec![
        CommandType::DseeSetParam as u8,
        0x01,
        if enabled { 0x01 } else { 0x00 },
    ]
}

// ============================================================================
// NC/ASM Inquiry Commands
// ============================================================================

/// Build command to get current NC/ASM settings
pub fn build_nc_asm_inquiry() -> Vec<u8> {
    vec![
        CommandType::NcAsmGetParam as u8,
        NcAsmInquiredType::NoiseCancellingAndAmbientSoundMode as i8 as u8,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nc_on_command() {
        let cmd = build_noise_cancelling_on();
        assert_eq!(cmd[0], CommandType::NcAsmSetParam as u8);
        assert_eq!(cmd[2], NcAsmEffect::On as i8 as u8);
    }

    #[test]
    fn test_ambient_sound_command() {
        let cmd = build_ambient_sound(10, false);
        assert_eq!(cmd[0], CommandType::NcAsmSetParam as u8);
        assert_eq!(cmd[7], 10); // Level
        assert_eq!(cmd[6], AsmId::Normal as i8 as u8);
    }

    #[test]
    fn test_ambient_sound_voice_focus() {
        let cmd = build_ambient_sound(10, true);
        assert_eq!(cmd[6], AsmId::Voice as i8 as u8);
    }

    #[test]
    fn test_vpt_preset() {
        let cmd = build_vpt_preset(VptPresetId::ConcertHall);
        assert_eq!(cmd[0], CommandType::VptSetParam as u8);
        assert_eq!(cmd[1], VptInquiredType::Vpt as i8 as u8);
        assert_eq!(cmd[2], VptPresetId::ConcertHall as i8 as u8);
    }

    // XM6-specific tests based on Bluetooth HCI traffic capture
    // Byte positions SWAPPED: position 4 = ASM mode, position 5 = NC mode
    #[test]
    fn test_xm6_nc_on_command() {
        // NC ON: 68:19:01:01:00:01:14:00:00 (enable=01, asm=00, nc=01)
        let cmd = build_anc_command_xm6(AncMode::NoiseCancelling);
        assert_eq!(cmd[0], 0x68); // NcAsmSetParam
        assert_eq!(cmd[1], 0x19); // XM6 inquired type
        assert_eq!(cmd[2], 0x01); // Sub-type
        assert_eq!(cmd[3], 0x01); // Enable (NC active)
        assert_eq!(cmd[4], 0x00); // ASM mode off (SWAPPED)
        assert_eq!(cmd[5], 0x01); // NC mode on (SWAPPED)
        assert_eq!(cmd[6], 0x14); // Level (20)
        assert_eq!(cmd[7], 0x00); // Focus on voice off
        assert_eq!(cmd[8], 0x00); // Reserved
    }

    #[test]
    fn test_xm6_nc_off_command() {
        // OFF: 68:19:01:00:00:00:14:00:00 (enable=00, asm=00, nc=00)
        let cmd = build_anc_command_xm6(AncMode::Off);
        assert_eq!(cmd[0], 0x68); // NcAsmSetParam
        assert_eq!(cmd[1], 0x19); // XM6 inquired type
        assert_eq!(cmd[2], 0x01); // Sub-type
        assert_eq!(cmd[3], 0x00); // Enable (all off)
        assert_eq!(cmd[4], 0x00); // ASM mode off
        assert_eq!(cmd[5], 0x00); // NC mode off
        assert_eq!(cmd[6], 0x14); // Level (20)
        assert_eq!(cmd[7], 0x00); // Focus on voice off
        assert_eq!(cmd[8], 0x00); // Reserved
    }

    #[test]
    fn test_xm6_ambient_sound_command() {
        // ASM ON: 68:19:01:01:01:00:0A:01:00 (enable=01, asm=01, nc=00, level=10, voice=01)
        let cmd = build_anc_command_xm6(AncMode::AmbientSound { level: 10, focus_on_voice: true });
        assert_eq!(cmd[0], 0x68); // NcAsmSetParam
        assert_eq!(cmd[1], 0x19); // XM6 inquired type
        assert_eq!(cmd[2], 0x01); // Sub-type
        assert_eq!(cmd[3], 0x01); // Enable (ASM active)
        assert_eq!(cmd[4], 0x01); // ASM mode on (SWAPPED)
        assert_eq!(cmd[5], 0x00); // NC mode off (SWAPPED)
        assert_eq!(cmd[6], 10);   // ASM level
        assert_eq!(cmd[7], 0x01); // Focus on voice on
        assert_eq!(cmd[8], 0x00); // Reserved
    }
}
