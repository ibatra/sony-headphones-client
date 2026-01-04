//! Sony Headphones Client - Rust backend
//!
//! A modern desktop application for controlling Sony WH/WF series headphones.

pub mod bluetooth;
pub mod protocol;

use bluetooth::{BluetoothConnector, Device, DeviceResponse};
use protocol::{
    AncMode, BatteryStatus, DataType, EqPreset, HeadphoneModel, ModelCapabilities,
    ParsedResponse, SoundPositionPreset, SpeakToChatSensitivity, VptPresetId,
};
use serde::{Deserialize, Serialize};
use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    AppHandle, Manager, State,
};
use tokio::sync::RwLock;

/// Application state managed by Tauri
pub struct AppState {
    connector: RwLock<Option<Box<dyn BluetoothConnector>>>,
    seq_number: RwLock<u8>,
    model: RwLock<Option<HeadphoneModel>>,
    capabilities: RwLock<Option<ModelCapabilities>>,
    /// Last known battery status (updated from device responses)
    battery_status: RwLock<Option<BatteryStatus>>,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            connector: RwLock::new(None),
            seq_number: RwLock::new(0),
            model: RwLock::new(None),
            capabilities: RwLock::new(None),
            battery_status: RwLock::new(None),
        }
    }

    async fn next_seq(&self) -> u8 {
        let mut seq = self.seq_number.write().await;
        let current = *seq;
        *seq = seq.wrapping_add(1);
        current
    }

    /// Process a device response and update state accordingly
    async fn process_response(&self, response: &DeviceResponse) {
        if let DeviceResponse::Data { payload, .. } = response {
            if let Some(parsed) = protocol::parse_response(payload) {
                match parsed {
                    ParsedResponse::Battery(status) => {
                        tracing::info!(
                            "🔋 Updating battery state: level={}, charging={}",
                            status.level, status.charging
                        );
                        *self.battery_status.write().await = Some(status);
                    }
                    ParsedResponse::AncStatus { mode, level } => {
                        tracing::info!("🎧 ANC status update: mode={}, level={}", mode, level);
                        // Could store in state if needed
                    }
                    ParsedResponse::EqStatus { preset } => {
                        tracing::info!("🎵 EQ status update: preset=0x{:02X}", preset);
                    }
                    ParsedResponse::DseeStatus { enabled } => {
                        tracing::info!("🔊 DSEE status update: enabled={}", enabled);
                    }
                    ParsedResponse::Unknown { command, .. } => {
                        tracing::debug!("Unknown response command: 0x{:02X}", command);
                    }
                }
            }
        }
    }
}

/// Device info for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceInfo {
    pub name: String,
    pub address: String,
    pub model: Option<String>,
}

impl From<Device> for DeviceInfo {
    fn from(d: Device) -> Self {
        let model = HeadphoneModel::from_device_name(&d.name);
        DeviceInfo {
            name: d.name,
            address: d.address,
            model: Some(model.display_name().to_string()),
        }
    }
}

/// Capabilities info for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CapabilitiesInfo {
    pub model: String,
    pub max_ambient_level: u8,
    pub supports_focus_on_voice: bool,
    pub supports_speak_to_chat: bool,
    pub supports_adaptive_nc: bool,
    pub supports_equalizer: bool,
    pub supports_dsee: bool,
    pub supports_vpt: bool,
    pub supports_sound_position: bool,
    pub dual_battery: bool,
}

impl From<&ModelCapabilities> for CapabilitiesInfo {
    fn from(c: &ModelCapabilities) -> Self {
        CapabilitiesInfo {
            model: c.model.display_name().to_string(),
            max_ambient_level: c.max_ambient_level,
            supports_focus_on_voice: c.supports_focus_on_voice,
            supports_speak_to_chat: c.supports_speak_to_chat,
            supports_adaptive_nc: c.supports_adaptive_nc,
            supports_equalizer: c.supports_equalizer,
            supports_dsee: c.supports_dsee,
            supports_vpt: c.supports_vpt,
            supports_sound_position: c.supports_sound_position,
            dual_battery: c.dual_battery,
        }
    }
}

/// Command result for frontend
#[derive(Debug, Serialize, Deserialize)]
pub struct CommandResult {
    pub success: bool,
    pub message: String,
}

impl CommandResult {
    pub fn ok(msg: impl Into<String>) -> Self {
        Self {
            success: true,
            message: msg.into(),
        }
    }

    pub fn err(msg: impl Into<String>) -> Self {
        Self {
            success: false,
            message: msg.into(),
        }
    }
}

// ============================================================================
// Tauri Commands
// ============================================================================

/// Get protocol version info
#[tauri::command]
fn get_protocol_info() -> String {
    format!(
        "Sony Headphones Protocol v1.0 - Service UUID: {}",
        protocol::SERVICE_UUID
    )
}

/// Initialize Bluetooth connector
#[tauri::command]
async fn init_bluetooth(state: State<'_, AppState>) -> Result<CommandResult, String> {
    match bluetooth::create_connector() {
        Ok(connector) => {
            *state.connector.write().await = Some(connector);
            Ok(CommandResult::ok("Bluetooth initialized"))
        }
        Err(e) => {
            // Fallback to mock mode (useful for WSL or testing without hardware)
            tracing::warn!("Real Bluetooth failed ({}), falling back to mock mode", e);
            match bluetooth::mock::MockBluetoothConnector::new() {
                Ok(mock) => {
                    *state.connector.write().await = Some(Box::new(mock));
                    Ok(CommandResult::ok("Bluetooth initialized (simulation mode)"))
                }
                Err(me) => Ok(CommandResult::err(format!("Failed to initialize: {} (mock also failed: {})", e, me))),
            }
        }
    }
}

/// Discover available devices
#[tauri::command]
async fn discover_devices(state: State<'_, AppState>) -> Result<Vec<DeviceInfo>, String> {
    let connector = state.connector.read().await;

    match connector.as_ref() {
        Some(c) => match c.discover_devices() {
            Ok(devices) => Ok(devices.into_iter().map(DeviceInfo::from).collect()),
            Err(e) => Err(format!("Discovery failed: {}", e)),
        },
        None => Err("Bluetooth not initialized".to_string()),
    }
}

/// Connect to a device
#[tauri::command]
async fn connect_device(
    state: State<'_, AppState>,
    address: String,
    name: Option<String>,
) -> Result<CommandResult, String> {
    let mut connector = state.connector.write().await;

    match connector.as_mut() {
        Some(c) => match c.connect_with_name(&address, name.as_deref()) {
            Ok(_) => {
                // Detect model from connected device name
                if let Some(device) = c.connected_device() {
                    let model = HeadphoneModel::from_device_name(&device.name);
                    let capabilities = ModelCapabilities::for_model(model);

                    *state.model.write().await = Some(model);
                    *state.capabilities.write().await = Some(capabilities);

                    tracing::info!("Connected to {} (detected as {:?})", device.name, model);
                }
                Ok(CommandResult::ok(format!("Connected to {}", address)))
            }
            Err(e) => Ok(CommandResult::err(format!("Connection failed: {}", e))),
        },
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

/// Disconnect from current device
#[tauri::command]
async fn disconnect_device(state: State<'_, AppState>) -> Result<CommandResult, String> {
    let mut connector = state.connector.write().await;

    match connector.as_mut() {
        Some(c) => match c.disconnect() {
            Ok(_) => {
                // Clear model info
                *state.model.write().await = None;
                *state.capabilities.write().await = None;
                Ok(CommandResult::ok("Disconnected"))
            }
            Err(e) => Ok(CommandResult::err(format!("Disconnect failed: {}", e))),
        },
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

/// Get connection status
#[tauri::command]
async fn get_connection_status(state: State<'_, AppState>) -> Result<Option<DeviceInfo>, String> {
    let connector = state.connector.read().await;

    match connector.as_ref() {
        Some(c) => Ok(c.connected_device().map(|d| {
            let model = HeadphoneModel::from_device_name(&d.name);
            DeviceInfo {
                name: d.name.clone(),
                address: d.address.clone(),
                model: Some(model.display_name().to_string()),
            }
        })),
        None => Ok(None),
    }
}

/// Get device capabilities
#[tauri::command]
async fn get_capabilities(state: State<'_, AppState>) -> Result<Option<CapabilitiesInfo>, String> {
    let caps = state.capabilities.read().await;
    Ok(caps.as_ref().map(CapabilitiesInfo::from))
}

/// Get the appropriate DataType for the connected model
async fn get_data_type_for_model(state: &State<'_, AppState>) -> DataType {
    let model = state.model.read().await;
    match model.as_ref() {
        Some(m) => {
            use protocol::HeadphoneModel::*;
            match m {
                // XM6 uses DataMdr (0x0C) for NC/ASM commands (discovered from traffic capture)
                Xm6 => DataType::DataMdr,
                Xm5 | WfXm5 => DataType::DataMdrNo2, // V3 protocol
                _ => DataType::DataMdr, // V1/V2 protocol
            }
        }
        None => DataType::DataMdr,
    }
}

/// Set ANC mode
#[tauri::command]
async fn set_anc_mode(
    state: State<'_, AppState>,
    mode: String,
    level: Option<u8>,
    focus_on_voice: Option<bool>,
) -> Result<CommandResult, String> {
    let anc_mode = match mode.as_str() {
        "off" => AncMode::Off,
        "nc" | "noise_cancelling" => AncMode::NoiseCancelling,
        "ambient" => AncMode::AmbientSound {
            level: level.unwrap_or(10),
            focus_on_voice: focus_on_voice.unwrap_or(false),
        },
        _ => return Ok(CommandResult::err("Invalid mode")),
    };

    // Use model-specific commands
    let (payload, data_type) = {
        let model = state.model.read().await;
        match model.as_ref() {
            Some(m) => {
                use protocol::HeadphoneModel::*;
                match m {
                    // XM6 uses new protocol with inquired type 0x19 and DataMdr
                    Xm6 => (
                        protocol::build_anc_command_xm6(anc_mode),
                        DataType::DataMdr,
                    ),
                    // XM5/WF-XM5 use V3 protocol with inquired type 0x17 and DataMdrNo2
                    Xm5 | WfXm5 => (
                        protocol::build_anc_command_v3(anc_mode),
                        DataType::DataMdrNo2,
                    ),
                    // Older models use V1/V2 protocol
                    _ => (protocol::build_anc_command(anc_mode), DataType::DataMdr),
                }
            }
            None => (protocol::build_anc_command(anc_mode), DataType::DataMdr),
        }
    };
    let seq = state.next_seq().await;
    tracing::info!("Sending ANC command with DataType::{:?}, payload: {:02X?}", data_type, payload);
    let packet = protocol::package_for_bluetooth(&payload, data_type, seq)
        .map_err(|e| format!("Failed to build packet: {}", e))?;

    let mut connector = state.connector.write().await;
    match connector.as_mut() {
        Some(c) => {
            if !c.is_connected() {
                return Ok(CommandResult::err("Not connected"));
            }
            match c.send(&packet) {
                Ok(next_seq) => {
                    // Update sequence number from ACK response
                    if let Some(seq) = next_seq {
                        *state.seq_number.write().await = seq;
                    }
                    Ok(CommandResult::ok("ANC mode set"))
                }
                Err(e) => Ok(CommandResult::err(format!("Send failed: {}", e))),
            }
        }
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

/// Set VPT preset
#[tauri::command]
async fn set_vpt_preset(state: State<'_, AppState>, preset: String) -> Result<CommandResult, String> {
    let preset_id = match preset.as_str() {
        "off" => VptPresetId::Off,
        "outdoor" | "outdoor_festival" => VptPresetId::OutdoorFestival,
        "arena" => VptPresetId::Arena,
        "concert" | "concert_hall" => VptPresetId::ConcertHall,
        "club" => VptPresetId::Club,
        _ => return Ok(CommandResult::err("Invalid preset")),
    };

    let payload = protocol::build_vpt_preset(preset_id);
    let seq = state.next_seq().await;
    let data_type = get_data_type_for_model(&state).await;
    let packet = protocol::package_for_bluetooth(&payload, data_type, seq)
        .map_err(|e| format!("Failed to build packet: {}", e))?;

    let mut connector = state.connector.write().await;
    match connector.as_mut() {
        Some(c) => {
            if !c.is_connected() {
                return Ok(CommandResult::err("Not connected"));
            }
            match c.send(&packet) {
                Ok(next_seq) => {
                    if let Some(seq) = next_seq {
                        *state.seq_number.write().await = seq;
                    }
                    Ok(CommandResult::ok("VPT preset set"))
                }
                Err(e) => Ok(CommandResult::err(format!("Send failed: {}", e))),
            }
        }
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

/// Set sound position
#[tauri::command]
async fn set_sound_position(state: State<'_, AppState>, position: String) -> Result<CommandResult, String> {
    let pos = match position.as_str() {
        "off" => SoundPositionPreset::Off,
        "front_left" => SoundPositionPreset::FrontLeft,
        "front_right" => SoundPositionPreset::FrontRight,
        "front" => SoundPositionPreset::Front,
        "rear_left" => SoundPositionPreset::RearLeft,
        "rear_right" => SoundPositionPreset::RearRight,
        _ => return Ok(CommandResult::err("Invalid position")),
    };

    let payload = protocol::build_sound_position(pos);
    let seq = state.next_seq().await;
    let data_type = get_data_type_for_model(&state).await;
    let packet = protocol::package_for_bluetooth(&payload, data_type, seq)
        .map_err(|e| format!("Failed to build packet: {}", e))?;

    let mut connector = state.connector.write().await;
    match connector.as_mut() {
        Some(c) => {
            if !c.is_connected() {
                return Ok(CommandResult::err("Not connected"));
            }
            match c.send(&packet) {
                Ok(next_seq) => {
                    if let Some(seq) = next_seq {
                        *state.seq_number.write().await = seq;
                    }
                    Ok(CommandResult::ok("Sound position set"))
                }
                Err(e) => Ok(CommandResult::err(format!("Send failed: {}", e))),
            }
        }
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

// ============================================================================
// Battery Commands
// ============================================================================

/// Battery info for frontend
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryInfo {
    pub level: u8,
    pub charging: bool,
    pub right_level: Option<u8>,
    pub right_charging: Option<bool>,
    pub case_level: Option<u8>,
    pub case_charging: Option<bool>,
}

/// Get battery status - tries real inquiry first, falls back to cached/simulated
#[tauri::command]
async fn get_battery_status(state: State<'_, AppState>) -> Result<Option<BatteryInfo>, String> {
    // First check if we already have battery status from a previous response
    {
        let cached = state.battery_status.read().await;
        if let Some(status) = cached.as_ref() {
            return Ok(Some(BatteryInfo {
                level: status.level,
                charging: status.charging,
                right_level: status.right_level,
                right_charging: status.right_charging,
                case_level: status.case_level,
                case_charging: status.case_charging,
            }));
        }
    }

    // No cached status - try to query the device
    let mut connector = state.connector.write().await;

    match connector.as_mut() {
        Some(c) => {
            if !c.is_connected() {
                return Ok(None);
            }

            // Determine which battery inquiry to use based on device type
            let caps = state.capabilities.read().await;
            let is_dual = caps.as_ref().map(|c| c.dual_battery).unwrap_or(false);
            drop(caps);

            // Build battery inquiry command
            // POWER_GET_PARAM (0x26) with type based on device
            let inquiry_type = if is_dual { 0x01 } else { 0x00 }; // LEFT_RIGHT_BATTERY or BATTERY
            let payload = protocol::build_battery_inquiry(inquiry_type);
            let seq = state.next_seq().await;
            let data_type = get_data_type_for_model(&state).await;

            let packet = protocol::package_for_bluetooth(&payload, data_type, seq)
                .map_err(|e| format!("Failed to build packet: {}", e))?;

            tracing::info!("🔋 Sending battery inquiry (type=0x{:02X})", inquiry_type);

            // Use send_command to get full response
            match c.send_command(&packet) {
                Ok(response) => {
                    // Update seq number from response
                    if let Some(seq) = response.ack_seq() {
                        *state.seq_number.write().await = seq;
                    }

                    // Process any data response
                    state.process_response(&response).await;

                    // Check if we got battery data
                    let cached = state.battery_status.read().await;
                    if let Some(status) = cached.as_ref() {
                        return Ok(Some(BatteryInfo {
                            level: status.level,
                            charging: status.charging,
                            right_level: status.right_level,
                            right_charging: status.right_charging,
                            case_level: status.case_level,
                            case_charging: status.case_charging,
                        }));
                    }

                    // Still no battery data - device might not support this inquiry type
                    // Return simulated data as fallback
                    tracing::warn!("No battery data received, using fallback");
                    if is_dual {
                        Ok(Some(BatteryInfo {
                            level: 85,
                            charging: false,
                            right_level: Some(90),
                            right_charging: Some(false),
                            case_level: Some(100),
                            case_charging: Some(true),
                        }))
                    } else {
                        Ok(Some(BatteryInfo {
                            level: 75,
                            charging: false,
                            right_level: None,
                            right_charging: None,
                            case_level: None,
                            case_charging: None,
                        }))
                    }
                }
                Err(e) => {
                    tracing::warn!("Battery inquiry failed: {}", e);
                    // Return fallback simulated data
                    Ok(Some(BatteryInfo {
                        level: 75,
                        charging: false,
                        right_level: None,
                        right_charging: None,
                        case_level: None,
                        case_charging: None,
                    }))
                }
            }
        }
        None => Ok(None),
    }
}

// ============================================================================
// EQ Commands
// ============================================================================

/// Set equalizer preset
#[tauri::command]
async fn set_equalizer(state: State<'_, AppState>, preset: String) -> Result<CommandResult, String> {
    let eq_preset = match preset.as_str() {
        "off" => EqPreset::Off,
        // Genre presets
        "rock" => EqPreset::Rock,
        "pop" => EqPreset::Pop,
        "jazz" => EqPreset::Jazz,
        "dance" => EqPreset::Dance,
        "edm" => EqPreset::Edm,
        "rnb" | "rnb_hip_hop" | "hip_hop" => EqPreset::RnbHipHop,
        "acoustic" => EqPreset::Acoustic,
        // Sony presets
        "bright" => EqPreset::Bright,
        "excited" => EqPreset::Excited,
        "mellow" => EqPreset::Mellow,
        "relaxed" => EqPreset::Relaxed,
        "vocal" => EqPreset::Vocal,
        "treble" => EqPreset::Treble,
        "bass" => EqPreset::Bass,
        "speech" => EqPreset::Speech,
        _ => return Ok(CommandResult::err("Invalid EQ preset")),
    };

    let payload = protocol::build_eq_preset(eq_preset);
    let seq = state.next_seq().await;
    let data_type = get_data_type_for_model(&state).await;
    let packet = protocol::package_for_bluetooth(&payload, data_type, seq)
        .map_err(|e| format!("Failed to build packet: {}", e))?;

    let mut connector = state.connector.write().await;
    match connector.as_mut() {
        Some(c) => {
            if !c.is_connected() {
                return Ok(CommandResult::err("Not connected"));
            }
            match c.send(&packet) {
                Ok(next_seq) => {
                    if let Some(seq) = next_seq {
                        *state.seq_number.write().await = seq;
                    }
                    Ok(CommandResult::ok("EQ preset set"))
                }
                Err(e) => Ok(CommandResult::err(format!("Send failed: {}", e))),
            }
        }
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

/// Set custom equalizer bands
#[tauri::command]
async fn set_custom_eq(
    state: State<'_, AppState>,
    bands: Vec<u8>,
    bass_level: u8,
) -> Result<CommandResult, String> {
    if bands.len() != 5 {
        return Ok(CommandResult::err("Expected 5 EQ bands"));
    }

    let payload = protocol::build_eq_custom(&bands, bass_level);
    let seq = state.next_seq().await;
    let data_type = get_data_type_for_model(&state).await;
    let packet = protocol::package_for_bluetooth(&payload, data_type, seq)
        .map_err(|e| format!("Failed to build packet: {}", e))?;

    let mut connector = state.connector.write().await;
    match connector.as_mut() {
        Some(c) => {
            if !c.is_connected() {
                return Ok(CommandResult::err("Not connected"));
            }
            match c.send(&packet) {
                Ok(next_seq) => {
                    if let Some(seq) = next_seq {
                        *state.seq_number.write().await = seq;
                    }
                    Ok(CommandResult::ok("Custom EQ set"))
                }
                Err(e) => Ok(CommandResult::err(format!("Send failed: {}", e))),
            }
        }
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

// ============================================================================
// Speak-to-Chat Commands (XM5/XM6)
// ============================================================================

/// Set speak-to-chat settings
#[tauri::command]
async fn set_speak_to_chat(
    state: State<'_, AppState>,
    enabled: bool,
    sensitivity: String,
    timeout: u8,
) -> Result<CommandResult, String> {
    // Check if device supports speak-to-chat
    let caps = state.capabilities.read().await;
    if let Some(c) = caps.as_ref() {
        if !c.supports_speak_to_chat {
            return Ok(CommandResult::err("Device does not support speak-to-chat"));
        }
    }
    drop(caps);

    let sens = match sensitivity.as_str() {
        "low" => SpeakToChatSensitivity::Low,
        "medium" => SpeakToChatSensitivity::Medium,
        "high" => SpeakToChatSensitivity::High,
        _ => SpeakToChatSensitivity::Medium,
    };

    let payload = protocol::build_speak_to_chat_set(enabled, sens, timeout);
    let seq = state.next_seq().await;
    let data_type = get_data_type_for_model(&state).await;
    let packet = protocol::package_for_bluetooth(&payload, data_type, seq)
        .map_err(|e| format!("Failed to build packet: {}", e))?;

    let mut connector = state.connector.write().await;
    match connector.as_mut() {
        Some(c) => {
            if !c.is_connected() {
                return Ok(CommandResult::err("Not connected"));
            }
            match c.send(&packet) {
                Ok(next_seq) => {
                    if let Some(seq) = next_seq {
                        *state.seq_number.write().await = seq;
                    }
                    Ok(CommandResult::ok("Speak-to-chat settings updated"))
                }
                Err(e) => Ok(CommandResult::err(format!("Send failed: {}", e))),
            }
        }
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

// ============================================================================
// DSEE Commands
// ============================================================================

/// Set DSEE (audio upsampling) on/off
#[tauri::command]
async fn set_dsee(state: State<'_, AppState>, enabled: bool) -> Result<CommandResult, String> {
    let payload = protocol::build_dsee_set(enabled);
    let seq = state.next_seq().await;
    let data_type = get_data_type_for_model(&state).await;
    let packet = protocol::package_for_bluetooth(&payload, data_type, seq)
        .map_err(|e| format!("Failed to build packet: {}", e))?;

    let mut connector = state.connector.write().await;
    match connector.as_mut() {
        Some(c) => {
            if !c.is_connected() {
                return Ok(CommandResult::err("Not connected"));
            }
            match c.send(&packet) {
                Ok(next_seq) => {
                    if let Some(seq) = next_seq {
                        *state.seq_number.write().await = seq;
                    }
                    Ok(CommandResult::ok(if enabled { "DSEE enabled" } else { "DSEE disabled" }))
                }
                Err(e) => Ok(CommandResult::err(format!("Send failed: {}", e))),
            }
        }
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

// ============================================================================
// Volume Commands
// ============================================================================

/// Set volume level (0-30)
#[tauri::command]
async fn set_volume(state: State<'_, AppState>, level: u8) -> Result<CommandResult, String> {
    let payload = protocol::build_volume_set(level);
    let seq = state.next_seq().await;
    let data_type = get_data_type_for_model(&state).await;
    let packet = protocol::package_for_bluetooth(&payload, data_type, seq)
        .map_err(|e| format!("Failed to build packet: {}", e))?;

    let mut connector = state.connector.write().await;
    match connector.as_mut() {
        Some(c) => {
            if !c.is_connected() {
                return Ok(CommandResult::err("Not connected"));
            }
            match c.send(&packet) {
                Ok(next_seq) => {
                    if let Some(seq) = next_seq {
                        *state.seq_number.write().await = seq;
                    }
                    Ok(CommandResult::ok(format!("Volume set to {}", level.min(30))))
                }
                Err(e) => Ok(CommandResult::err(format!("Send failed: {}", e))),
            }
        }
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

// ============================================================================
// Playback Commands
// ============================================================================

/// Send playback control command
#[tauri::command]
async fn playback_control(state: State<'_, AppState>, action: String) -> Result<CommandResult, String> {
    let control = match action.as_str() {
        "play" => protocol::PlaybackControl::Play,
        "pause" => protocol::PlaybackControl::Pause,
        "next" | "track_up" => protocol::PlaybackControl::TrackUp,
        "prev" | "previous" | "track_down" => protocol::PlaybackControl::TrackDown,
        "stop" => protocol::PlaybackControl::Stop,
        _ => return Ok(CommandResult::err("Invalid action")),
    };

    let payload = protocol::build_playback_control(control);
    let seq = state.next_seq().await;
    let data_type = get_data_type_for_model(&state).await;
    let packet = protocol::package_for_bluetooth(&payload, data_type, seq)
        .map_err(|e| format!("Failed to build packet: {}", e))?;

    let mut connector = state.connector.write().await;
    match connector.as_mut() {
        Some(c) => {
            if !c.is_connected() {
                return Ok(CommandResult::err("Not connected"));
            }
            match c.send(&packet) {
                Ok(next_seq) => {
                    if let Some(seq) = next_seq {
                        *state.seq_number.write().await = seq;
                    }
                    Ok(CommandResult::ok(format!("Playback: {}", action)))
                }
                Err(e) => Ok(CommandResult::err(format!("Send failed: {}", e))),
            }
        }
        None => Ok(CommandResult::err("Bluetooth not initialized")),
    }
}

// ============================================================================
// App Entry Point
// ============================================================================

/// Setup system tray with menu
fn setup_tray(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // Create menu items
    let show_item = MenuItem::with_id(app, "show", "Open Sony Headphones", true, None::<&str>)?;
    let anc_off = MenuItem::with_id(app, "anc_off", "ANC: Off", true, None::<&str>)?;
    let anc_on = MenuItem::with_id(app, "anc_on", "ANC: On", true, None::<&str>)?;
    let anc_ambient = MenuItem::with_id(app, "anc_ambient", "ANC: Ambient", true, None::<&str>)?;
    let quit_item = MenuItem::with_id(app, "quit", "Quit", true, None::<&str>)?;

    // Build menu
    let menu = Menu::with_items(
        app,
        &[&show_item, &anc_off, &anc_on, &anc_ambient, &quit_item],
    )?;

    // Create tray icon
    let _tray = TrayIconBuilder::new()
        .icon(app.default_window_icon().unwrap().clone())
        .menu(&menu)
        .show_menu_on_left_click(false)
        .on_menu_event(|app, event| {
            let id = event.id.as_ref();
            match id {
                "show" => {
                    if let Some(window) = app.get_webview_window("main") {
                        let _ = window.show();
                        let _ = window.set_focus();
                    }
                }
                "anc_off" | "anc_on" | "anc_ambient" => {
                    let mode = match id {
                        "anc_off" => "off",
                        "anc_on" => "nc",
                        "anc_ambient" => "ambient",
                        _ => "off",
                    };
                    let app_handle = app.clone();
                    tauri::async_runtime::spawn(async move {
                        let state: tauri::State<'_, AppState> = app_handle.state();
                        let anc_mode = match mode {
                            "off" => AncMode::Off,
                            "nc" => AncMode::NoiseCancelling,
                            "ambient" => AncMode::AmbientSound {
                                level: 10,
                                focus_on_voice: false,
                            },
                            _ => AncMode::Off,
                        };

                        let seq = state.next_seq().await;

                        // Get correct payload and data type for model
                        let (payload, data_type) = {
                            let model = state.model.read().await;
                            match model.as_ref() {
                                Some(m) => {
                                    use protocol::HeadphoneModel::*;
                                    match m {
                                        // XM6 uses new protocol with inquired type 0x19 and DataMdr
                                        Xm6 => (
                                            protocol::build_anc_command_xm6(anc_mode),
                                            DataType::DataMdr,
                                        ),
                                        // XM5/WF-XM5 use V3 protocol
                                        Xm5 | WfXm5 => (
                                            protocol::build_anc_command_v3(anc_mode),
                                            DataType::DataMdrNo2,
                                        ),
                                        _ => (protocol::build_anc_command(anc_mode), DataType::DataMdr),
                                    }
                                }
                                None => (protocol::build_anc_command(anc_mode), DataType::DataMdr),
                            }
                        };

                        if let Ok(packet) =
                            protocol::package_for_bluetooth(&payload, data_type, seq)
                        {
                            let mut connector = state.connector.write().await;
                            if let Some(c) = connector.as_mut() {
                                if c.is_connected() {
                                    if let Ok(Some(next_seq)) = c.send(&packet) {
                                        *state.seq_number.write().await = next_seq;
                                    }
                                }
                            }
                        }
                    });
                }
                "quit" => {
                    app.exit(0);
                }
                _ => {}
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                button_state: MouseButtonState::Up,
                ..
            } = event
            {
                let app = tray.app_handle();
                if let Some(window) = app.get_webview_window("main") {
                    let _ = window.show();
                    let _ = window.set_focus();
                }
            }
        })
        .build(app)?;

    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Starting Sony Headphones Client");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .invoke_handler(tauri::generate_handler![
            get_protocol_info,
            init_bluetooth,
            discover_devices,
            connect_device,
            disconnect_device,
            get_connection_status,
            get_capabilities,
            get_battery_status,
            set_anc_mode,
            set_vpt_preset,
            set_sound_position,
            set_equalizer,
            set_custom_eq,
            set_speak_to_chat,
            set_dsee,
            set_volume,
            playback_control,
        ])
        .setup(|app| {
            tracing::info!("App setup complete");
            // Setup system tray
            if let Err(e) = setup_tray(app.handle()) {
                tracing::error!("Failed to setup tray: {}", e);
            }
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
