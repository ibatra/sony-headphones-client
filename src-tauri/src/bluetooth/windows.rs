//! Windows Bluetooth implementation using Winsock2 BTH
//!
//! Uses Windows Bluetooth APIs for device discovery and Winsock BTH for RFCOMM.

use super::{BluetoothConnector, BluetoothError, BluetoothResult, Device};
use std::mem::{size_of, zeroed};
use windows::core::GUID;
use windows::Win32::Devices::Bluetooth::{
    BluetoothFindDeviceClose, BluetoothFindFirstDevice, BluetoothFindNextDevice,
    BLUETOOTH_DEVICE_INFO, BLUETOOTH_DEVICE_SEARCH_PARAMS,
};
use windows::Win32::Networking::WinSock::{
    closesocket, WSACleanup, WSAGetLastError, WSAStartup, INVALID_SOCKET,
    SOCKET, SOCK_STREAM, WSADATA,
};

// Bluetooth-specific constants not exposed by the windows crate
const AF_BTH: i32 = 32;
const BTHPROTO_RFCOMM: i32 = 3;

// Socket option constants for receive timeout
const SOL_SOCKET: i32 = 0xFFFF;
const SO_RCVTIMEO: i32 = 0x1006;

// Protocol markers for ACK messages
const START_MARKER: u8 = 0x3E; // '>'
const END_MARKER: u8 = 0x3C;   // '<'

/// Sony headphones service UUID
const SONY_UUID: GUID = GUID::from_values(
    0x96CC203E,
    0x5068,
    0x46AD,
    [0xB3, 0x2D, 0xE3, 0x16, 0xF5, 0xE0, 0x69, 0xBA],
);

/// Bluetooth socket address structure for Windows
/// Must match Windows SOCKADDR_BTH exactly - packed structure, 30 bytes total
#[repr(C, packed)]
#[derive(Clone, Copy)]
struct SOCKADDR_BTH {
    address_family: u16,   // offset 0, 2 bytes
    bt_addr: u64,          // offset 2, 8 bytes (no padding in packed struct)
    service_class_id: GUID, // offset 10, 16 bytes
    port: u32,             // offset 26, 4 bytes
}                          // Total: 30 bytes

// Compile-time verification that SOCKADDR_BTH has the correct size
const _: () = assert!(size_of::<SOCKADDR_BTH>() == 30, "SOCKADDR_BTH must be exactly 30 bytes");

// Raw FFI for Bluetooth socket functions
#[link(name = "ws2_32")]
extern "system" {
    fn socket(af: i32, socket_type: i32, protocol: i32) -> SOCKET;
    fn connect(s: SOCKET, name: *const SOCKADDR_BTH, namelen: i32) -> i32;
    fn send(s: SOCKET, buf: *const u8, len: i32, flags: i32) -> i32;
    fn recv(s: SOCKET, buf: *mut u8, len: i32, flags: i32) -> i32;
    fn setsockopt(s: SOCKET, level: i32, optname: i32, optval: *const u8, optlen: i32) -> i32;
}

/// Windows Bluetooth connector using Winsock2 BTH
pub struct WindowsBluetoothConnector {
    socket: SOCKET,
    connected_device: Option<Device>,
    wsa_initialized: bool,
}

impl WindowsBluetoothConnector {
    /// Create a new Windows Bluetooth connector
    pub fn new() -> BluetoothResult<Self> {
        // Initialize Winsock
        let mut wsa_data: WSADATA = unsafe { zeroed() };
        let result = unsafe { WSAStartup(0x0202, &mut wsa_data) };
        if result != 0 {
            return Err(BluetoothError::ConnectionFailed(format!(
                "WSAStartup failed with error: {}",
                result
            )));
        }

        Ok(Self {
            socket: INVALID_SOCKET,
            connected_device: None,
            wsa_initialized: true,
        })
    }

    /// Parse Bluetooth address from string (e.g., "00:11:22:33:44:55")
    fn parse_address(address: &str) -> BluetoothResult<u64> {
        let parts: Vec<&str> = address.split(':').collect();
        if parts.len() != 6 {
            return Err(BluetoothError::ConnectionFailed("Invalid address format".to_string()));
        }

        let mut addr: u64 = 0;
        for (i, part) in parts.iter().enumerate() {
            let byte = u8::from_str_radix(part, 16)
                .map_err(|_| BluetoothError::ConnectionFailed("Invalid address".to_string()))?;
            addr |= (byte as u64) << ((5 - i) * 8);
        }
        Ok(addr)
    }

    /// Format Bluetooth address to string
    fn format_address(addr: u64) -> String {
        format!(
            "{:02X}:{:02X}:{:02X}:{:02X}:{:02X}:{:02X}",
            (addr >> 40) & 0xFF,
            (addr >> 32) & 0xFF,
            (addr >> 24) & 0xFF,
            (addr >> 16) & 0xFF,
            (addr >> 8) & 0xFF,
            addr & 0xFF
        )
    }
}

impl Drop for WindowsBluetoothConnector {
    fn drop(&mut self) {
        if self.socket != INVALID_SOCKET {
            unsafe { closesocket(self.socket) };
        }
        if self.wsa_initialized {
            unsafe { WSACleanup() };
        }
    }
}

impl WindowsBluetoothConnector {
    /// Internal connect implementation
    fn connect_internal(&mut self, address: &str, device_name: Option<&str>) -> BluetoothResult<()> {
        if self.socket != INVALID_SOCKET {
            unsafe { closesocket(self.socket) };
        }

        // Create Bluetooth socket using raw FFI
        let sock = unsafe { socket(AF_BTH, SOCK_STREAM.0 as i32, BTHPROTO_RFCOMM) };
        if sock == INVALID_SOCKET {
            let err = unsafe { WSAGetLastError() };
            return Err(BluetoothError::ConnectionFailed(format!(
                "Failed to create socket: {}",
                err.0
            )));
        }

        // Parse address
        let bt_addr = Self::parse_address(address)?;

        // Set up socket address
        // Try connecting via UUID first, then fall back to specific channels
        let mut sock_addr: SOCKADDR_BTH = unsafe { zeroed() };
        sock_addr.address_family = AF_BTH as u16;
        sock_addr.bt_addr = bt_addr;
        sock_addr.service_class_id = SONY_UUID;
        sock_addr.port = 0; // Try SDP lookup first

        tracing::info!("Attempting connection to {} via SDP lookup", address);

        // Connect using raw FFI
        let mut result = unsafe {
            connect(
                sock,
                &sock_addr,
                size_of::<SOCKADDR_BTH>() as i32,
            )
        };

        // If SDP lookup fails, try specific RFCOMM channels
        if result != 0 {
            // Close the original socket that failed SDP lookup
            unsafe { closesocket(sock) };

            let channels_to_try = [9, 5, 1, 2, 3, 4, 6, 7, 8];
            for channel in channels_to_try {
                tracing::info!("Trying RFCOMM channel {} for {}", channel, address);

                // Create a new socket for each attempt
                let sock_new = unsafe { socket(AF_BTH, SOCK_STREAM.0 as i32, BTHPROTO_RFCOMM) };
                if sock_new == INVALID_SOCKET {
                    continue;
                }

                // Clear UUID and set specific channel
                sock_addr.service_class_id = GUID::from_values(0, 0, 0, [0; 8]);
                sock_addr.port = channel;

                result = unsafe {
                    connect(
                        sock_new,
                        &sock_addr,
                        size_of::<SOCKADDR_BTH>() as i32,
                    )
                };

                if result == 0 {
                    self.socket = sock_new;
                    let name = device_name
                        .map(|s| s.to_string())
                        .unwrap_or_else(|| format!("Sony Headphones ({})", address));
                    self.connected_device = Some(Device {
                        name,
                        address: address.to_string(),
                    });
                    tracing::info!("Connected to {} on channel {}", address, channel);
                    return Ok(());
                }

                unsafe { closesocket(sock_new) };
            }

            let err = unsafe { WSAGetLastError() };
            return Err(BluetoothError::ConnectionFailed(format!(
                "Failed to connect to {} - tried SDP and channels 1-9 (last error {})",
                address, err.0
            )));
        }

        // SDP lookup succeeded
        self.socket = sock;
        let name = device_name
            .map(|s| s.to_string())
            .unwrap_or_else(|| format!("Sony Headphones ({})", address));
        self.connected_device = Some(Device {
            name,
            address: address.to_string(),
        });

        tracing::info!("Connected to {} via SDP", address);
        Ok(())
    }

    /// Wait for ACK response from headphones with timeout
    /// The protocol uses START_MARKER ('>') and END_MARKER ('<') for message framing
    /// Returns the next sequence number from the ACK (if received)
    ///
    /// ACK format after unescaping: [data_type, seq_num, size[4], checksum]
    /// - data_type 0x01 = ACK
    /// - seq_num = next sequence number to use
    fn wait_for_ack(&mut self) -> BluetoothResult<Option<u8>> {
        const TIMEOUT_MS: u32 = 1000;
        const ACK_DATA_TYPE: u8 = 0x01;

        // Set receive timeout on socket
        let timeout_val: i32 = TIMEOUT_MS as i32;
        let result = unsafe {
            setsockopt(
                self.socket,
                SOL_SOCKET,
                SO_RCVTIMEO,
                &timeout_val as *const i32 as *const u8,
                std::mem::size_of::<i32>() as i32,
            )
        };
        if result != 0 {
            tracing::warn!("Failed to set socket timeout");
        }

        let mut buffer = Vec::new();
        let mut found_start = false;
        let mut total_received = 0;

        // Read until we get a complete message (END_MARKER) or timeout
        loop {
            let mut chunk = [0u8; 256];
            let n = unsafe {
                recv(
                    self.socket,
                    chunk.as_mut_ptr(),
                    chunk.len() as i32,
                    0,
                )
            };

            if n <= 0 {
                // Timeout (WSAETIMEDOUT = 10060) or error
                let err = unsafe { WSAGetLastError() };
                if err.0 == 10060 {
                    tracing::debug!("ACK timeout after {} bytes", total_received);
                } else if err.0 != 0 {
                    tracing::warn!("ACK receive error: {}", err.0);
                }
                break;
            }

            total_received += n as usize;

            // Parse received bytes looking for message frame
            for &byte in &chunk[..n as usize] {
                if byte == START_MARKER {
                    found_start = true;
                    buffer.clear();
                } else if byte == END_MARKER && found_start {
                    // Complete message received - parse it
                    // Buffer format: [data_type, seq_num, size[4], checksum]
                    // Note: data may be escaped, but ACK messages typically don't contain escape-needing bytes
                    if buffer.len() >= 2 {
                        let data_type = buffer[0];
                        let next_seq = buffer[1];

                        if data_type == ACK_DATA_TYPE {
                            tracing::info!("ACK received: next_seq={}, {} bytes total", next_seq, total_received);
                            return Ok(Some(next_seq));
                        } else {
                            // Not an ACK, might be a data response - log and continue
                            tracing::debug!("Received non-ACK response: type=0x{:02X}, {} bytes", data_type, buffer.len());
                        }
                    }
                    return Ok(None);
                } else if found_start {
                    buffer.push(byte);
                }
            }

            // Safety limit - don't read forever
            if total_received > 4096 {
                tracing::warn!("ACK response too large, stopping");
                break;
            }
        }

        // Don't fail if no ACK - some commands may not require it
        Ok(None)
    }
}

impl BluetoothConnector for WindowsBluetoothConnector {
    fn discover_devices(&self) -> BluetoothResult<Vec<Device>> {
        let mut devices = Vec::new();

        // Set up search parameters
        let mut search_params: BLUETOOTH_DEVICE_SEARCH_PARAMS = unsafe { zeroed() };
        search_params.dwSize = size_of::<BLUETOOTH_DEVICE_SEARCH_PARAMS>() as u32;
        search_params.fReturnAuthenticated = true.into();
        search_params.fReturnRemembered = true.into();
        search_params.fReturnConnected = true.into();
        search_params.fReturnUnknown = true.into();
        search_params.fIssueInquiry = true.into();
        search_params.cTimeoutMultiplier = 4; // ~5 seconds

        // Set up device info structure
        let mut device_info: BLUETOOTH_DEVICE_INFO = unsafe { zeroed() };
        device_info.dwSize = size_of::<BLUETOOTH_DEVICE_INFO>() as u32;

        // Find first device
        let find_result = unsafe { BluetoothFindFirstDevice(&search_params, &mut device_info) };

        let handle = match find_result {
            Ok(h) => h,
            Err(_) => return Ok(devices), // No devices found
        };

        if handle.is_invalid() {
            return Ok(devices);
        }

        loop {
            // Get device name from wide string
            let name_len = device_info
                .szName
                .iter()
                .position(|&c| c == 0)
                .unwrap_or(device_info.szName.len());
            let name = String::from_utf16_lossy(&device_info.szName[..name_len]);

            // Filter for Sony headphones
            let name_lower = name.to_lowercase();

            // Skip BLE-only devices (they have "LE_" prefix and won't work for RFCOMM)
            let is_le_only = name_lower.starts_with("le_") || name_lower.starts_with("le-");

            if !is_le_only && (name_lower.contains("wh-1000xm")
                || name_lower.contains("wf-1000xm")
                || name_lower.contains("sony"))
            {
                let address = Self::format_address(unsafe { device_info.Address.Anonymous.ullLong });

                // Only skip exact address duplicates
                let already_exists = devices.iter().any(|d| d.address == address);
                if !already_exists {
                    // Add address suffix to help distinguish multiple entries
                    let display_name = if devices.iter().any(|d| d.name == name) {
                        format!("{} [{}]", name, &address[address.len()-5..])
                    } else {
                        name
                    };
                    devices.push(Device { name: display_name, address });
                }
            }

            // Find next device
            let has_next = unsafe { BluetoothFindNextDevice(handle, &mut device_info) };
            if has_next.is_err() {
                break;
            }
        }

        let _ = unsafe { BluetoothFindDeviceClose(handle) };

        Ok(devices)
    }

    fn connect(&mut self, address: &str) -> BluetoothResult<()> {
        self.connect_internal(address, None)
    }

    fn connect_with_name(&mut self, address: &str, name: Option<&str>) -> BluetoothResult<()> {
        self.connect_internal(address, name)
    }

    fn disconnect(&mut self) -> BluetoothResult<()> {
        if self.socket != INVALID_SOCKET {
            unsafe { closesocket(self.socket) };
            self.socket = INVALID_SOCKET;
        }
        self.connected_device = None;
        tracing::info!("Disconnected");
        Ok(())
    }

    fn send(&mut self, data: &[u8]) -> BluetoothResult<Option<u8>> {
        if self.socket == INVALID_SOCKET {
            return Err(BluetoothError::NotConnected);
        }

        let result = unsafe {
            send(
                self.socket,
                data.as_ptr(),
                data.len() as i32,
                0,
            )
        };

        if result < 0 {
            let err = unsafe { WSAGetLastError() };
            // Connection-related errors - mark socket as disconnected
            // 10053 = WSAECONNABORTED, 10054 = WSAECONNRESET, 10057 = WSAENOTCONN
            if err.0 == 10053 || err.0 == 10054 || err.0 == 10057 {
                tracing::warn!("Connection lost (error {}), marking as disconnected", err.0);
                unsafe { closesocket(self.socket) };
                self.socket = INVALID_SOCKET;
                self.connected_device = None;
                return Err(BluetoothError::NotConnected);
            }
            return Err(BluetoothError::SendFailed(format!("Send failed: {}", err.0)));
        }

        tracing::debug!("Sent {} bytes", data.len());

        // Wait for ACK response from headphones (blocking with timeout)
        // This is required to keep the protocol in sync - the C++ implementation
        // also blocks waiting for ACK after each command
        // Returns the next sequence number from the ACK
        self.wait_for_ack()
    }

    fn receive(&mut self) -> BluetoothResult<Vec<u8>> {
        if self.socket == INVALID_SOCKET {
            return Err(BluetoothError::NotConnected);
        }

        let mut buffer = vec![0u8; 2048];
        let result = unsafe {
            recv(
                self.socket,
                buffer.as_mut_ptr(),
                buffer.len() as i32,
                0,
            )
        };

        if result < 0 {
            let err = unsafe { WSAGetLastError() };
            return Err(BluetoothError::ReceiveFailed(format!(
                "Receive failed: {}",
                err.0
            )));
        }

        buffer.truncate(result as usize);
        tracing::debug!("Received {} bytes", buffer.len());
        Ok(buffer)
    }

    fn is_connected(&self) -> bool {
        self.socket != INVALID_SOCKET && self.connected_device.is_some()
    }

    fn connected_device(&self) -> Option<&Device> {
        self.connected_device.as_ref()
    }
}
