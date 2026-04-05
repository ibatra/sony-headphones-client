//! Sony Headphones Client - Background RFCOMM Service
//!
//! Runs as a background service that maintains an RFCOMM control channel
//! to Sony WH/WF headphones. The RFCOMM channel being open is what activates
//! gesture/touch controls (ANC toggle, volume swipe, wear detection, etc.).
//!
//! Without an active RFCOMM control channel, macOS connects via A2DP/HFP
//! for audio but gestures remain disabled.

pub mod bluetooth;
pub mod protocol;

use bluetooth::BluetoothConnector;
use protocol::{HeadphoneModel, ModelCapabilities};
use std::sync::atomic::{AtomicBool, Ordering};
use tauri::{AppHandle, Manager};
use tokio::sync::RwLock;

// ============================================================================
// Application State
// ============================================================================

pub struct AppState {
    connector: RwLock<Option<Box<dyn BluetoothConnector>>>,
    model: RwLock<Option<HeadphoneModel>>,
    capabilities: RwLock<Option<ModelCapabilities>>,
    /// Whether the background service loop is running
    service_running: AtomicBool,
}

impl AppState {
    pub fn new() -> Self {
        Self {
            connector: RwLock::new(None),
            model: RwLock::new(None),
            capabilities: RwLock::new(None),
            service_running: AtomicBool::new(false),
        }
    }
}

// ============================================================================
// Background Service Loop
// ============================================================================

// No initialization commands are sent after RFCOMM connection.
// Testing showed that simply having an open RFCOMM channel is sufficient
// to activate gesture/touch controls on the XM6. Sending protocol commands
// (battery inquiry, NC/ASM query, etc.) actually caused the headphones to
// close the channel ~22s later — the XM6 uses SPP and interprets our
// DataMdr-framed commands as invalid traffic.

/// The main background service that:
/// 1. Discovers and connects to Sony headphones via RFCOMM
/// 2. Keeps the RFCOMM channel open (this is what enables gestures)
/// 3. Reconnects automatically if the connection drops
fn start_service(app_handle: AppHandle) {
    let state: tauri::State<'_, AppState> = app_handle.state();
    if state.service_running.swap(true, Ordering::SeqCst) {
        tracing::info!("Service already running, skipping");
        return;
    }

    let app = app_handle.clone();
    tauri::async_runtime::spawn(async move {
        let state: tauri::State<'_, AppState> = app.state();

        // Wait on startup to let macOS establish normal Bluetooth connections
        // (A2DP audio, HFP hands-free) before we open our RFCOMM channel.
        // Without this delay, our RFCOMM connects first and macOS shows the
        // headphones as "connected" but doesn't route audio through them.
        tracing::info!("Waiting 15s for macOS to establish audio connection...");
        tokio::time::sleep(std::time::Duration::from_secs(15)).await;

        // Reconnect backoff: after each short-lived connection (<60s),
        // increase the wait before retrying. The headphones need a settle
        // period (~30s) after Bluetooth connects before RFCOMM is stable.
        let mut reconnect_wait_secs: u64 = 2;

        loop {
            // Phase 1: Ensure we have a Bluetooth connector
            {
                let conn = state.connector.read().await;
                if conn.is_none() {
                    drop(conn);
                    match bluetooth::create_connector() {
                        Ok(connector) => {
                            *state.connector.write().await = Some(connector);
                            tracing::info!("Bluetooth initialized");
                        }
                        Err(e) => {
                            tracing::warn!("Bluetooth init failed: {}, retrying in 10s", e);
                            tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                            continue;
                        }
                    }
                }
            }

            // Phase 2: Check if already connected
            let is_connected = {
                let conn = state.connector.read().await;
                conn.as_ref().map(|c| c.is_connected()).unwrap_or(false)
            };

            if !is_connected {
                // Phase 3: Discover and connect
                tracing::info!("Scanning for Sony headphones...");
                let target = {
                    let conn = state.connector.read().await;
                    match conn.as_ref() {
                        Some(c) => match c.discover_devices() {
                            Ok(devices) => {
                                if devices.is_empty() {
                                    tracing::info!("No Sony headphones found, retrying in 5s");
                                    None
                                } else {
                                    Some((devices[0].name.clone(), devices[0].address.clone()))
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Discovery failed: {}, retrying in 5s", e);
                                None
                            }
                        },
                        None => None,
                    }
                };

                let (name, address) = match target {
                    Some(t) => t,
                    None => {
                        tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                        continue;
                    }
                };

                tracing::info!("Found {}, connecting...", name);

                let connected = {
                    let mut conn = state.connector.write().await;
                    if let Some(c) = conn.as_mut() {
                        match c.connect_with_name(&address, Some(&name)) {
                            Ok(_) => {
                                if let Some(device) = c.connected_device() {
                                    let model = HeadphoneModel::from_device_name(&device.name);
                                    let capabilities = ModelCapabilities::for_model(model);

                                    tracing::info!(
                                        "Connected to {} (model: {:?})",
                                        device.name, model
                                    );

                                    *state.model.write().await = Some(model);
                                    *state.capabilities.write().await = Some(capabilities);
                                    true
                                } else {
                                    false
                                }
                            }
                            Err(e) => {
                                tracing::warn!("Connection failed: {}", e);
                                false
                            }
                        }
                    } else {
                        false
                    }
                };

                if !connected {
                    tracing::info!("Will retry in 10s");
                    tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                    continue;
                }

                // No init commands — just having the RFCOMM channel open enables gestures.
                tracing::info!("RFCOMM channel open — gestures should be active");
            }

            // Phase 4: Monitor connection and drain incoming notifications.
            // The headphones send notifications over RFCOMM (playback state,
            // ANC changes, etc.) and expect ACKs. Without timely ACKs the
            // headphones re-transmit and gesture state gets out of sync.
            // Poll every 200ms for responsive ACKing.
            let connected_at = std::time::Instant::now();

            loop {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;

                // Drain and ACK any pending notifications from the headphones
                {
                    let mut conn = state.connector.write().await;
                    if let Some(c) = conn.as_mut() {
                        let _ = c.drain_notifications();
                    }
                }

                let still_connected = {
                    let conn = state.connector.read().await;
                    conn.as_ref().map(|c| c.is_connected()).unwrap_or(false)
                };

                if !still_connected {
                    let uptime = connected_at.elapsed();
                    *state.model.write().await = None;
                    *state.capabilities.write().await = None;

                    if uptime.as_secs() >= 60 {
                        // Was stable — reconnect quickly
                        reconnect_wait_secs = 2;
                        tracing::info!(
                            "Connection lost after {}s (was stable), reconnecting in 2s...",
                            uptime.as_secs()
                        );
                        tracing::info!("Reconnecting...");
                        tokio::time::sleep(std::time::Duration::from_secs(2)).await;
                    } else {
                        // Short-lived — headphones still settling, back off
                        tracing::info!(
                            "Connection closed after {}s (settling), waiting {}s before retry...",
                            uptime.as_secs(), reconnect_wait_secs
                        );
                        tracing::info!("Waiting for headphones to settle...");
                        tokio::time::sleep(std::time::Duration::from_secs(reconnect_wait_secs)).await;
                        // Increase backoff: 2 -> 5 -> 10 -> 15 -> 15 (cap)
                        reconnect_wait_secs = (reconnect_wait_secs + 5).min(15);
                    }
                    break; // Back to outer loop to reconnect
                }

                // Connection stable — if we've been up >60s, reset backoff
                if connected_at.elapsed().as_secs() >= 60 && reconnect_wait_secs > 2 {
                    tracing::info!("Connection stable for 60s, resetting backoff");
                    reconnect_wait_secs = 2;
                }
            }
        }
    });
}

// ============================================================================
// App Entry Point
// ============================================================================

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("Starting Sony Headphones Background Service");

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::new())
        .setup(|app| {
            // No windows — pure background service (LSUIElement=true hides Dock icon)
            // Start the background service (connect + keep-alive)
            start_service(app.handle().clone());
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
