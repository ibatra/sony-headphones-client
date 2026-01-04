//! Linux Bluetooth implementation using BlueZ RFCOMM
//!
//! This module provides Bluetooth RFCOMM connectivity on Linux using:
//! - D-Bus for device discovery (BlueZ)
//! - Direct RFCOMM sockets via libc for data transfer

use super::{BluetoothConnector, BluetoothError, BluetoothResult, Device};
use dbus::arg::RefArg;
use std::os::unix::io::RawFd;

// BlueZ constants
const BTPROTO_RFCOMM: libc::c_int = 3;
const AF_BLUETOOTH: libc::c_int = 31;

// Protocol markers for message framing
const START_MARKER: u8 = 0x3E; // '>'
const END_MARKER: u8 = 0x3C;   // '<'
const ACK_DATA_TYPE: u8 = 0x01;

// RFCOMM socket address structure
#[repr(C)]
struct SockaddrRc {
    rc_family: libc::sa_family_t,
    rc_bdaddr: [u8; 6],
    rc_channel: u8,
}

/// Linux Bluetooth connector using BlueZ
pub struct LinuxBluetoothConnector {
    socket: Option<RawFd>,
    connected_device: Option<Device>,
}

impl LinuxBluetoothConnector {
    /// Create a new Linux Bluetooth connector
    pub fn new() -> BluetoothResult<Self> {
        Ok(Self {
            socket: None,
            connected_device: None,
        })
    }

    /// Parse MAC address string to bytes
    fn parse_mac_address(address: &str) -> BluetoothResult<[u8; 6]> {
        let parts: Vec<&str> = address.split(':').collect();
        if parts.len() != 6 {
            return Err(BluetoothError::ConnectionFailed(format!(
                "Invalid MAC address format: {}",
                address
            )));
        }

        let mut bytes = [0u8; 6];
        for (i, part) in parts.iter().enumerate() {
            bytes[5 - i] = u8::from_str_radix(part, 16).map_err(|_| {
                BluetoothError::ConnectionFailed(format!("Invalid MAC address: {}", address))
            })?;
        }
        Ok(bytes)
    }

    /// Get RFCOMM channel for Sony headphones service using SDP
    fn get_rfcomm_channel(&self, _address: &str) -> BluetoothResult<u8> {
        // For now, use a common default channel
        // TODO: Implement proper SDP lookup
        // Sony headphones typically use channel 9 or similar
        Ok(9)
    }

    /// Send an ACK packet back to the device
    /// This is required by the bidirectional protocol - client must ACK device responses
    fn send_ack(&self, fd: RawFd, received_seq: u8) -> BluetoothResult<()> {
        let next_seq = 1 - received_seq;
        let ack_payload = vec![0x01, next_seq, 0, 0, 0, 0]; // DATA_TYPE::ACK = 0x01
        let checksum: u8 = ack_payload.iter().fold(0u8, |a, b| a.wrapping_add(*b));

        let mut packet = vec![START_MARKER];
        packet.extend(&ack_payload);
        packet.push(checksum);
        packet.push(END_MARKER);

        let result = unsafe {
            libc::send(
                fd,
                packet.as_ptr() as *const libc::c_void,
                packet.len(),
                0,
            )
        };

        if result < 0 {
            tracing::warn!("Failed to send ACK: {}", std::io::Error::last_os_error());
        } else {
            tracing::debug!("Sent ACK (seq={})", next_seq);
        }

        Ok(())
    }

    /// Wait for ACK response from headphones with timeout
    fn wait_for_ack(&self, fd: RawFd) -> BluetoothResult<Option<u8>> {
        // Set receive timeout
        let timeout = libc::timeval {
            tv_sec: 1,
            tv_usec: 0,
        };
        unsafe {
            libc::setsockopt(
                fd,
                libc::SOL_SOCKET,
                libc::SO_RCVTIMEO,
                &timeout as *const libc::timeval as *const libc::c_void,
                std::mem::size_of::<libc::timeval>() as libc::socklen_t,
            );
        }

        let mut buffer = Vec::new();
        let mut found_start = false;
        let mut total_received = 0;

        loop {
            let mut chunk = [0u8; 256];
            let n = unsafe {
                libc::recv(
                    fd,
                    chunk.as_mut_ptr() as *mut libc::c_void,
                    chunk.len(),
                    0,
                )
            };

            if n <= 0 {
                // Timeout or error
                if n < 0 {
                    let err = std::io::Error::last_os_error();
                    if err.kind() == std::io::ErrorKind::WouldBlock || err.kind() == std::io::ErrorKind::TimedOut {
                        tracing::debug!("ACK timeout after {} bytes", total_received);
                    } else {
                        tracing::warn!("ACK receive error: {}", err);
                    }
                }
                break;
            }

            total_received += n as usize;

            for &byte in &chunk[..n as usize] {
                if byte == START_MARKER {
                    found_start = true;
                    buffer.clear();
                } else if byte == END_MARKER && found_start {
                    if buffer.len() >= 2 {
                        let data_type = buffer[0];
                        let next_seq = buffer[1];

                        if data_type == ACK_DATA_TYPE {
                            tracing::info!("ACK received: next_seq={}, {} bytes total", next_seq, total_received);
                            // Send ACK back to device (bidirectional protocol requirement)
                            self.send_ack(fd, next_seq)?;
                            return Ok(Some(next_seq));
                        } else {
                            tracing::debug!("Received non-ACK response: type=0x{:02X}, {} bytes", data_type, buffer.len());
                        }
                    }
                    return Ok(None);
                } else if found_start {
                    buffer.push(byte);
                }
            }

            if total_received > 4096 {
                tracing::warn!("ACK response too large, stopping");
                break;
            }
        }

        Ok(None)
    }

    /// Discover devices using D-Bus (BlueZ)
    fn discover_via_dbus(&self) -> BluetoothResult<Vec<Device>> {
        // Connect to D-Bus system bus
        let conn = dbus::blocking::Connection::new_system()
            .map_err(|e| BluetoothError::ConnectionFailed(format!("D-Bus error: {}", e)))?;

        let proxy = conn.with_proxy(
            "org.bluez",
            "/",
            std::time::Duration::from_secs(5),
        );

        // Use GetManagedObjects to list all Bluetooth devices
        use dbus::blocking::stdintf::org_freedesktop_dbus::ObjectManager;
        let objects = proxy
            .get_managed_objects()
            .map_err(|e| BluetoothError::ConnectionFailed(format!("D-Bus error: {}", e)))?;

        let mut devices = Vec::new();

        for (_path, interfaces) in objects {
            if let Some(device_props) = interfaces.get("org.bluez.Device1") {
                let name: Option<String> = device_props
                    .get("Name")
                    .and_then(|v| v.0.as_str())
                    .map(|s| s.to_string());

                let address: Option<String> = device_props
                    .get("Address")
                    .and_then(|v| v.0.as_str())
                    .map(|s| s.to_string());

                // Only include paired Sony devices
                if let (Some(name), Some(address)) = (name, address) {
                    if name.contains("WH-1000") || name.contains("WF-1000") || name.contains("MDR-") {
                        devices.push(Device { name, address });
                    }
                }
            }
        }

        Ok(devices)
    }
}

impl BluetoothConnector for LinuxBluetoothConnector {
    fn discover_devices(&self) -> BluetoothResult<Vec<Device>> {
        self.discover_via_dbus()
    }

    fn connect(&mut self, address: &str) -> BluetoothResult<()> {
        // Close existing connection if any
        if self.socket.is_some() {
            self.disconnect()?;
        }

        // Create RFCOMM socket
        let fd = unsafe { libc::socket(AF_BLUETOOTH, libc::SOCK_STREAM, BTPROTO_RFCOMM) };
        if fd < 0 {
            return Err(BluetoothError::ConnectionFailed(
                "Failed to create RFCOMM socket".to_string(),
            ));
        }

        // Parse MAC address
        let mac_bytes = Self::parse_mac_address(address)?;

        // Get RFCOMM channel
        let channel = self.get_rfcomm_channel(address)?;

        // Set up socket address
        let addr = SockaddrRc {
            rc_family: AF_BLUETOOTH as u16,
            rc_bdaddr: mac_bytes,
            rc_channel: channel,
        };

        // Connect
        let result = unsafe {
            libc::connect(
                fd,
                &addr as *const SockaddrRc as *const libc::sockaddr,
                std::mem::size_of::<SockaddrRc>() as libc::socklen_t,
            )
        };

        if result < 0 {
            unsafe { libc::close(fd) };
            return Err(BluetoothError::ConnectionFailed(format!(
                "Failed to connect to {}: {}",
                address,
                std::io::Error::last_os_error()
            )));
        }

        self.socket = Some(fd);
        self.connected_device = Some(Device {
            name: "Sony Headphones".to_string(),
            address: address.to_string(),
        });

        tracing::info!("Connected to {} on channel {}", address, channel);
        Ok(())
    }

    fn disconnect(&mut self) -> BluetoothResult<()> {
        if let Some(fd) = self.socket.take() {
            unsafe { libc::close(fd) };
        }
        self.connected_device = None;
        Ok(())
    }

    fn send(&mut self, data: &[u8]) -> BluetoothResult<Option<u8>> {
        let fd = self.socket.ok_or(BluetoothError::NotConnected)?;

        let sent = unsafe {
            libc::send(
                fd,
                data.as_ptr() as *const libc::c_void,
                data.len(),
                0,
            )
        };

        if sent < 0 {
            return Err(BluetoothError::SendFailed(
                std::io::Error::last_os_error().to_string(),
            ));
        }

        tracing::debug!("Sent {} bytes", sent);

        // Wait for ACK response and send ACK back (bidirectional protocol)
        self.wait_for_ack(fd)
    }

    fn receive(&mut self) -> BluetoothResult<Vec<u8>> {
        let fd = self.socket.ok_or(BluetoothError::NotConnected)?;

        let mut buffer = vec![0u8; 2048];

        let received = unsafe {
            libc::recv(
                fd,
                buffer.as_mut_ptr() as *mut libc::c_void,
                buffer.len(),
                0,
            )
        };

        if received < 0 {
            return Err(BluetoothError::ReceiveFailed(
                std::io::Error::last_os_error().to_string(),
            ));
        }

        buffer.truncate(received as usize);
        tracing::debug!("Received {} bytes", received);
        Ok(buffer)
    }

    fn connect_with_name(&mut self, address: &str, name: Option<&str>) -> BluetoothResult<()> {
        // Close existing connection if any
        if self.socket.is_some() {
            self.disconnect()?;
        }

        // Create RFCOMM socket
        let fd = unsafe { libc::socket(AF_BLUETOOTH, libc::SOCK_STREAM, BTPROTO_RFCOMM) };
        if fd < 0 {
            return Err(BluetoothError::ConnectionFailed(
                "Failed to create RFCOMM socket".to_string(),
            ));
        }

        // Parse MAC address
        let mac_bytes = Self::parse_mac_address(address)?;

        // Get RFCOMM channel
        let channel = self.get_rfcomm_channel(address)?;

        // Set up socket address
        let addr = SockaddrRc {
            rc_family: AF_BLUETOOTH as u16,
            rc_bdaddr: mac_bytes,
            rc_channel: channel,
        };

        // Connect
        let result = unsafe {
            libc::connect(
                fd,
                &addr as *const SockaddrRc as *const libc::sockaddr,
                std::mem::size_of::<SockaddrRc>() as libc::socklen_t,
            )
        };

        if result < 0 {
            unsafe { libc::close(fd) };
            return Err(BluetoothError::ConnectionFailed(format!(
                "Failed to connect to {}: {}",
                address,
                std::io::Error::last_os_error()
            )));
        }

        self.socket = Some(fd);
        // Use provided name if available, otherwise use generic name
        let device_name = name.map(|n| n.to_string()).unwrap_or_else(|| "Sony Headphones".to_string());
        self.connected_device = Some(Device {
            name: device_name,
            address: address.to_string(),
        });

        tracing::info!("Connected to {} on channel {}", address, channel);
        Ok(())
    }

    fn is_connected(&self) -> bool {
        self.socket.is_some()
    }

    fn connected_device(&self) -> Option<&Device> {
        self.connected_device.as_ref()
    }
}

impl Drop for LinuxBluetoothConnector {
    fn drop(&mut self) {
        let _ = self.disconnect();
    }
}
