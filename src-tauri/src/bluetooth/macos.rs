//! macOS Bluetooth implementation using IOBluetooth framework
//!
//! This module provides Bluetooth RFCOMM connectivity on macOS by calling
//! into an Objective-C bridge (macos_bridge.m) that wraps Apple's IOBluetooth
//! framework. The bridge handles:
//! - Device discovery via IOBluetoothDevice.pairedDevices
//! - RFCOMM channel management via IOBluetoothRFCOMMChannel
//! - Delegate-based async receive with condition-variable synchronization
//!
//! The protocol framing (START/END markers, ACK handling) is done in Rust,
//! consistent with the Windows and Linux implementations.

use super::{BluetoothConnector, BluetoothError, BluetoothResult, Device, DeviceResponse};
use std::ffi::{c_char, c_int, c_void, CString};

// Protocol markers for message framing (same as Windows/Linux)
const START_MARKER: u8 = 0x3E; // '>'
const END_MARKER: u8 = 0x3C; // '<'
const ACK_DATA_TYPE: u8 = 0x01;
const DATA_MDR: u8 = 0x0C;
const DATA_MDR_NO2: u8 = 0x0E;

/// Device info struct matching the C definition in macos_bridge.m
#[repr(C)]
#[derive(Clone)]
struct SonyBTDeviceInfo {
    name: [u8; 256],
    address: [u8; 18],
}

// FFI declarations for the Objective-C bridge
extern "C" {
    fn sony_bt_hide_from_dock();
    fn sony_bt_discover(out_devices: *mut SonyBTDeviceInfo, max_count: c_int) -> c_int;
    fn sony_bt_connect(address: *const c_char, out_error: *mut c_int) -> *mut c_void;
    fn sony_bt_send(connection: *mut c_void, data: *const u8, length: c_int) -> c_int;
    fn sony_bt_receive(
        connection: *mut c_void,
        buffer: *mut u8,
        buffer_size: c_int,
        timeout_ms: c_int,
    ) -> c_int;
    fn sony_bt_disconnect(connection: *mut c_void);
    fn sony_bt_is_connected(connection: *mut c_void) -> c_int;
}

/// macOS Bluetooth connector using IOBluetooth via Objective-C bridge
pub struct MacOSBluetoothConnector {
    connection: *mut c_void,
    connected_device: Option<Device>,
}

// Safety: The Objective-C bridge uses dispatch_sync to the main thread for
// IOBluetooth operations and NSCondition for cross-thread data synchronization.
unsafe impl Send for MacOSBluetoothConnector {}
unsafe impl Sync for MacOSBluetoothConnector {}

impl MacOSBluetoothConnector {
    /// Create a new macOS Bluetooth connector
    pub fn new() -> BluetoothResult<Self> {
        // Hide from Dock — we're a background service
        unsafe { sony_bt_hide_from_dock() };

        Ok(Self {
            connection: std::ptr::null_mut(),
            connected_device: None,
        })
    }

    /// Extract a null-terminated string from a fixed-size byte array
    fn str_from_fixed_bytes(bytes: &[u8]) -> String {
        let len = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
        String::from_utf8_lossy(&bytes[..len]).to_string()
    }

    /// Format bytes as hex dump for logging
    fn hex_dump(data: &[u8], max_bytes: usize) -> String {
        let len = data.len().min(max_bytes);
        let hex: Vec<String> = data[..len].iter().map(|b| format!("{:02X}", b)).collect();
        if data.len() > max_bytes {
            format!("[{}... ({} more)]", hex.join(" "), data.len() - max_bytes)
        } else {
            format!("[{}]", hex.join(" "))
        }
    }

    /// Send an ACK packet back to the device.
    /// Required by the bidirectional protocol - client must ACK device responses.
    /// ACK format: START + [0x01, 1-seq, 0,0,0,0, checksum] + END
    fn send_ack(&mut self, received_seq: u8) -> BluetoothResult<()> {
        let next_seq = 1 - received_seq;
        let ack_payload = vec![0x01, next_seq, 0, 0, 0, 0]; // DATA_TYPE::ACK = 0x01
        let checksum: u8 = ack_payload.iter().fold(0u8, |a, b| a.wrapping_add(*b));

        let mut packet = vec![START_MARKER];
        packet.extend(&ack_payload);
        packet.push(checksum);
        packet.push(END_MARKER);

        let result = unsafe {
            sony_bt_send(self.connection, packet.as_ptr(), packet.len() as c_int)
        };

        if result < 0 {
            tracing::warn!("Failed to send ACK");
        } else {
            tracing::debug!("Sent ACK (seq={})", next_seq);
        }

        Ok(())
    }

    /// Parse one complete framed message from a byte stream.
    /// Returns (data_type, seq, payload) if found, None on timeout/error.
    fn read_one_message(&mut self, timeout_ms: c_int) -> Option<(u8, u8, Vec<u8>)> {
        let mut buffer = Vec::new();
        let mut found_start = false;
        let mut total_received = 0;

        loop {
            let mut chunk = [0u8; 512];
            let n = unsafe {
                sony_bt_receive(
                    self.connection,
                    chunk.as_mut_ptr(),
                    chunk.len() as c_int,
                    timeout_ms,
                )
            };

            if n <= 0 {
                return None; // Timeout or error
            }

            total_received += n as usize;

            for &byte in &chunk[..n as usize] {
                if byte == START_MARKER {
                    found_start = true;
                    buffer.clear();
                } else if byte == END_MARKER && found_start {
                    if buffer.len() >= 2 {
                        let data_type = buffer[0];
                        let seq = buffer[1];
                        return Some((data_type, seq, buffer));
                    }
                    return None;
                } else if found_start {
                    buffer.push(byte);
                }
            }

            if total_received > 8192 {
                return None;
            }
        }
    }

    /// Drain any stale notifications from the receive buffer.
    /// Headset sends unsolicited notifications when the user presses physical buttons.
    /// If we don't drain these, they corrupt the next command's ACK response.
    fn drain_stale_notifications(&mut self) -> Vec<DeviceResponse> {
        let mut notifications = Vec::new();

        loop {
            // Non-blocking read (0ms timeout)
            match self.read_one_message(0) {
                Some((data_type, seq, payload)) => {
                    let type_name = match data_type {
                        ACK_DATA_TYPE => "ACK",
                        DATA_MDR => "DATA_MDR",
                        DATA_MDR_NO2 => "DATA_MDR_NO2",
                        _ => "UNKNOWN",
                    };
                    tracing::info!(
                        "Drained stale {}: seq={} {} bytes\n   Hex: {}",
                        type_name, seq, payload.len(),
                        Self::hex_dump(&payload, 32)
                    );

                    // ACK stale data/notifications so device doesn't re-send them
                    if data_type != ACK_DATA_TYPE {
                        let _ = self.send_ack(seq);
                        notifications.push(DeviceResponse::Data {
                            data_type,
                            seq,
                            payload,
                        });
                    }
                }
                None => break, // Buffer empty
            }
        }

        if !notifications.is_empty() {
            tracing::info!("Drained {} stale notification(s)", notifications.len());
        }

        notifications
    }

    /// Wait for the ACK response after sending a command.
    /// Skips past any unsolicited device notifications (from headset button presses)
    /// to find the actual ACK. Notifications are captured and returned for processing.
    fn wait_for_response(&mut self) -> BluetoothResult<(DeviceResponse, Vec<DeviceResponse>)> {
        const TIMEOUT_MS: c_int = 1500;
        const MAX_MESSAGES: usize = 10;
        let mut notifications = Vec::new();

        for attempt in 0..MAX_MESSAGES {
            match self.read_one_message(TIMEOUT_MS) {
                Some((data_type, seq, payload)) => {
                    let type_name = match data_type {
                        ACK_DATA_TYPE => "ACK",
                        DATA_MDR => "DATA_MDR",
                        DATA_MDR_NO2 => "DATA_MDR_NO2",
                        _ => "UNKNOWN",
                    };
                    tracing::info!(
                        "Response #{}: type=0x{:02X}({}) seq={} size={}\n   Hex: {}",
                        attempt + 1, data_type, type_name, seq, payload.len(),
                        Self::hex_dump(&payload, 48)
                    );

                    if data_type == ACK_DATA_TYPE {
                        // Found the ACK — send ACK back and return
                        self.send_ack(seq)?;
                        return Ok((
                            DeviceResponse::Ack { next_seq: seq },
                            notifications,
                        ));
                    } else {
                        // Data/notification — ACK it, save it, keep looking for ACK
                        self.send_ack(seq)?;
                        notifications.push(DeviceResponse::Data {
                            data_type,
                            seq,
                            payload,
                        });
                        // Continue reading for the actual ACK
                    }
                }
                None => {
                    // Timeout — no more messages
                    tracing::debug!("wait_for_response: timeout after {} messages", attempt);
                    return Ok((DeviceResponse::Timeout, notifications));
                }
            }
        }

        tracing::warn!("wait_for_response: hit max messages ({})", MAX_MESSAGES);
        Ok((DeviceResponse::Timeout, notifications))
    }

    /// Legacy wait_for_ack - wraps wait_for_response for backwards compatibility
    fn wait_for_ack(&mut self) -> BluetoothResult<Option<u8>> {
        let (response, _notifications) = self.wait_for_response()?;
        match response {
            DeviceResponse::Ack { next_seq } => Ok(Some(next_seq)),
            DeviceResponse::Data { seq, .. } => Ok(Some(seq)),
            DeviceResponse::Timeout => Ok(None),
        }
    }

    /// Internal connect implementation
    fn connect_internal(
        &mut self,
        address: &str,
        device_name: Option<&str>,
    ) -> BluetoothResult<()> {
        // Disconnect existing connection if any
        if !self.connection.is_null() {
            self.disconnect()?;
        }

        let c_address = CString::new(address)
            .map_err(|_| BluetoothError::ConnectionFailed("Invalid address".to_string()))?;

        let mut error: c_int = 0;
        let conn = unsafe { sony_bt_connect(c_address.as_ptr(), &mut error) };

        if conn.is_null() {
            let msg = match error {
                1 => format!("Device not found: {}", address),
                2 => format!(
                    "Failed to open RFCOMM channel to {} - tried SDP and channels 1-9",
                    address
                ),
                _ => format!("Connection failed (error {})", error),
            };
            return Err(BluetoothError::ConnectionFailed(msg));
        }

        self.connection = conn;
        let name = device_name
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Sony Headphones ({})", address));
        self.connected_device = Some(Device {
            name: name.clone(),
            address: address.to_string(),
        });

        tracing::info!("Connected to {} @ {}", name, address);
        Ok(())
    }
}

impl Drop for MacOSBluetoothConnector {
    fn drop(&mut self) {
        if !self.connection.is_null() {
            unsafe { sony_bt_disconnect(self.connection) };
            self.connection = std::ptr::null_mut();
        }
    }
}

impl BluetoothConnector for MacOSBluetoothConnector {
    fn discover_devices(&self) -> BluetoothResult<Vec<Device>> {
        const MAX_DEVICES: usize = 64;
        let mut raw_devices = vec![
            SonyBTDeviceInfo {
                name: [0u8; 256],
                address: [0u8; 18],
            };
            MAX_DEVICES
        ];

        let count =
            unsafe { sony_bt_discover(raw_devices.as_mut_ptr(), MAX_DEVICES as c_int) };

        let mut all_found = Vec::new();
        let mut devices = Vec::new();

        for i in 0..count as usize {
            let name = Self::str_from_fixed_bytes(&raw_devices[i].name);
            let address = Self::str_from_fixed_bytes(&raw_devices[i].address);

            let name_lower = name.to_lowercase();

            // Skip BLE-only devices
            let is_le_only = name_lower.starts_with("le_") || name_lower.starts_with("le-");

            let is_sony = name_lower.contains("wh-1000xm")
                || name_lower.contains("wf-1000xm")
                || name_lower.contains("sony");

            let device_type = if is_le_only { "BLE" } else { "Classic" };

            all_found.push(format!(
                "  {} [{}] @ {} {}",
                name,
                device_type,
                address,
                if is_sony { "<- Sony" } else { "" }
            ));

            if !is_le_only && is_sony {
                let already_exists = devices.iter().any(|d: &Device| d.address == address);
                if !already_exists {
                    let display_name = if devices.iter().any(|d: &Device| d.name == name) {
                        format!("{} [{}]", name, &address[address.len().saturating_sub(5)..])
                    } else {
                        name.clone()
                    };

                    tracing::info!("Adding device: {} @ {}", display_name, address);
                    devices.push(Device {
                        name: display_name,
                        address,
                    });
                }
            } else if is_le_only && is_sony {
                tracing::info!(
                    "Skipping BLE-only Sony device: {} @ {} (won't work for RFCOMM)",
                    name,
                    address
                );
            }
        }

        tracing::info!(
            "Discovery complete. Found {} Bluetooth devices:\n{}",
            all_found.len(),
            all_found.join("\n")
        );
        tracing::info!("Filtered to {} Sony devices for selection", devices.len());

        Ok(devices)
    }

    fn connect(&mut self, address: &str) -> BluetoothResult<()> {
        self.connect_internal(address, None)
    }

    fn connect_with_name(&mut self, address: &str, name: Option<&str>) -> BluetoothResult<()> {
        self.connect_internal(address, name)
    }

    fn disconnect(&mut self) -> BluetoothResult<()> {
        if !self.connection.is_null() {
            unsafe { sony_bt_disconnect(self.connection) };
            self.connection = std::ptr::null_mut();
        }
        self.connected_device = None;
        tracing::info!("Disconnected");
        Ok(())
    }

    fn send(&mut self, data: &[u8]) -> BluetoothResult<Option<u8>> {
        // Drain stale notifications from headset button presses before sending
        self.drain_stale_notifications();

        if self.connection.is_null() {
            return Err(BluetoothError::NotConnected);
        }

        tracing::info!(
            "Sending {} bytes:\n   Hex: {}",
            data.len(),
            Self::hex_dump(data, 48)
        );

        let result =
            unsafe { sony_bt_send(self.connection, data.as_ptr(), data.len() as c_int) };

        if result < 0 {
            let connected = unsafe { sony_bt_is_connected(self.connection) };
            if connected == 0 {
                tracing::warn!("Connection lost, marking as disconnected");
                self.connection = std::ptr::null_mut();
                self.connected_device = None;
                return Err(BluetoothError::NotConnected);
            }
            return Err(BluetoothError::SendFailed("writeSync failed".to_string()));
        }

        // Wait for ACK, skipping past any stale notifications
        let (response, _notifications) = self.wait_for_response()?;
        Ok(response.ack_seq())
    }

    fn send_command(&mut self, data: &[u8]) -> BluetoothResult<DeviceResponse> {
        // Drain stale notifications from headset button presses before sending
        let stale = self.drain_stale_notifications();

        if self.connection.is_null() {
            return Err(BluetoothError::NotConnected);
        }

        tracing::info!(
            "Sending command: {} bytes\n   Hex: {}",
            data.len(),
            Self::hex_dump(data, 48)
        );

        let result =
            unsafe { sony_bt_send(self.connection, data.as_ptr(), data.len() as c_int) };

        if result < 0 {
            let connected = unsafe { sony_bt_is_connected(self.connection) };
            if connected == 0 {
                tracing::warn!("Connection lost, marking as disconnected");
                self.connection = std::ptr::null_mut();
                self.connected_device = None;
                return Err(BluetoothError::NotConnected);
            }
            return Err(BluetoothError::SendFailed("writeSync failed".to_string()));
        }

        // Wait for ACK, skipping past any interleaved notifications
        let (ack_response, mut notifications) = self.wait_for_response()?;

        // If we got data notifications (from headset buttons), return the first one
        // so the caller can process it (e.g., update volume/ANC state)
        if !notifications.is_empty() {
            // Prepend any stale notifications we drained earlier
            for s in stale.into_iter().rev() {
                notifications.insert(0, s);
            }
            // Return the ACK if we got one, otherwise first notification
            if ack_response.is_ack() {
                return Ok(ack_response);
            }
            return Ok(notifications.remove(0));
        }

        Ok(ack_response)
    }

    fn receive(&mut self) -> BluetoothResult<Vec<u8>> {
        if self.connection.is_null() {
            return Err(BluetoothError::NotConnected);
        }

        let mut buffer = vec![0u8; 2048];
        let n = unsafe {
            sony_bt_receive(
                self.connection,
                buffer.as_mut_ptr(),
                buffer.len() as c_int,
                1000, // 1 second timeout
            )
        };

        if n < 0 {
            return Err(BluetoothError::ReceiveFailed(
                "Receive failed (connection lost?)".to_string(),
            ));
        }

        buffer.truncate(n as usize);
        tracing::debug!("Received {} bytes", buffer.len());
        Ok(buffer)
    }

    fn is_connected(&self) -> bool {
        if self.connection.is_null() {
            return false;
        }
        let connected = unsafe { sony_bt_is_connected(self.connection) };
        connected != 0 && self.connected_device.is_some()
    }

    fn connected_device(&self) -> Option<&Device> {
        self.connected_device.as_ref()
    }

    fn drain_notifications(&mut self) -> BluetoothResult<usize> {
        if self.connection.is_null() {
            return Ok(0);
        }

        // Read raw bytes from the buffer (non-blocking, 0ms timeout).
        let mut raw_buf = [0u8; 2048];
        let n = unsafe {
            sony_bt_receive(
                self.connection,
                raw_buf.as_mut_ptr(),
                raw_buf.len() as c_int,
                0, // non-blocking
            )
        };

        if n <= 0 {
            return Ok(0);
        }

        let data = &raw_buf[..n as usize];

        // Parse Sony protocol messages (START 3E ... END 3C) and ACK them.
        // The headphones re-transmit if we don't ACK fast enough, so we see
        // duplicate messages. Only ACK the FIRST copy of each message —
        // deduplicate by checking if we already ACKed this exact (data_type, seq) pair.
        let mut count = 0;
        let mut acked_in_this_batch: Vec<(u8, u8)> = Vec::new();
        let mut i = 0;

        while i < data.len() {
            if data[i] == START_MARKER {
                // Find the matching END_MARKER
                if let Some(end_offset) = data[i + 1..].iter().position(|&b| b == END_MARKER) {
                    let end = i + 1 + end_offset;
                    let inner = &data[i + 1..end];
                    if inner.len() >= 2 {
                        let data_type = inner[0];
                        let seq = inner[1];

                        if data_type != ACK_DATA_TYPE {
                            let key = (data_type, seq);
                            if !acked_in_this_batch.contains(&key) {
                                // First time seeing this (type, seq) — ACK it
                                let _ = self.send_ack(seq);
                                acked_in_this_batch.push(key);
                                count += 1;
                                tracing::debug!(
                                    "ACKed notification: type=0x{:02X} seq={}",
                                    data_type, seq
                                );
                            }
                            // else: duplicate, skip
                        }
                    }
                    i = end + 1;
                } else {
                    i += 1;
                }
            } else {
                i += 1;
            }
        }

        Ok(count)
    }
}
