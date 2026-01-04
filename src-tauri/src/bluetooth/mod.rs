//! Bluetooth abstraction layer
//!
//! Provides a cross-platform interface for Bluetooth RFCOMM communication
//! with Sony headphones.

#[cfg(target_os = "linux")]
pub mod linux;

#[cfg(target_os = "windows")]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

use std::fmt;
use thiserror::Error;

/// Bluetooth device information
#[derive(Debug, Clone)]
pub struct Device {
    pub name: String,
    pub address: String,
}

impl fmt::Display for Device {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} ({})", self.name, self.address)
    }
}

/// Bluetooth connection errors
#[derive(Debug, Error)]
pub enum BluetoothError {
    #[error("Device not found: {0}")]
    DeviceNotFound(String),

    #[error("Connection failed: {0}")]
    ConnectionFailed(String),

    #[error("Not connected")]
    NotConnected,

    #[error("Send failed: {0}")]
    SendFailed(String),

    #[error("Receive failed: {0}")]
    ReceiveFailed(String),

    #[error("Service not found")]
    ServiceNotFound,

    #[error("Platform not supported")]
    PlatformNotSupported,

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result type for Bluetooth operations
pub type BluetoothResult<T> = Result<T, BluetoothError>;

/// Bluetooth connector trait - platform implementations must implement this
pub trait BluetoothConnector: Send + Sync {
    /// Discover available Sony headphones
    fn discover_devices(&self) -> BluetoothResult<Vec<Device>>;

    /// Connect to a device by MAC address
    fn connect(&mut self, address: &str) -> BluetoothResult<()>;

    /// Connect with an optional device name (for proper model detection)
    fn connect_with_name(&mut self, address: &str, name: Option<&str>) -> BluetoothResult<()>;

    /// Disconnect from the current device
    fn disconnect(&mut self) -> BluetoothResult<()>;

    /// Send data to the connected device
    /// Returns the next sequence number from ACK response (if received)
    fn send(&mut self, data: &[u8]) -> BluetoothResult<Option<u8>>;

    /// Receive data from the connected device
    fn receive(&mut self) -> BluetoothResult<Vec<u8>>;

    /// Check if currently connected
    fn is_connected(&self) -> bool;

    /// Get the connected device info
    fn connected_device(&self) -> Option<&Device>;
}

/// Create a platform-specific Bluetooth connector
pub fn create_connector() -> BluetoothResult<Box<dyn BluetoothConnector>> {
    #[cfg(target_os = "linux")]
    {
        Ok(Box::new(linux::LinuxBluetoothConnector::new()?))
    }

    #[cfg(target_os = "windows")]
    {
        Ok(Box::new(windows::WindowsBluetoothConnector::new()?))
    }

    #[cfg(target_os = "macos")]
    {
        Ok(Box::new(macos::MacOSBluetoothConnector::new()?))
    }

    #[cfg(not(any(target_os = "linux", target_os = "windows", target_os = "macos")))]
    {
        Err(BluetoothError::PlatformNotSupported)
    }
}

/// Mock connector for testing without hardware (also used in WSL)
pub mod mock {
    use super::*;
    use crate::protocol::{self, DataType};

    pub struct MockBluetoothConnector {
        connected: bool,
        device: Option<Device>,
        seq_number: u8,
    }

    impl MockBluetoothConnector {
        pub fn new() -> BluetoothResult<Self> {
            Ok(Self {
                connected: false,
                device: None,
                seq_number: 0,
            })
        }
    }

    impl BluetoothConnector for MockBluetoothConnector {
        fn discover_devices(&self) -> BluetoothResult<Vec<Device>> {
            // Simulate finding Sony headphones
            Ok(vec![
                Device {
                    name: "WH-1000XM6 (Simulated)".to_string(),
                    address: "00:11:22:33:44:55".to_string(),
                },
                Device {
                    name: "WH-1000XM5 (Simulated)".to_string(),
                    address: "AA:BB:CC:DD:EE:FF".to_string(),
                },
                Device {
                    name: "WH-1000XM4 (Simulated)".to_string(),
                    address: "11:22:33:44:55:66".to_string(),
                },
            ])
        }

        fn connect(&mut self, address: &str) -> BluetoothResult<()> {
            self.connect_with_name(address, None)
        }

        fn connect_with_name(&mut self, address: &str, name: Option<&str>) -> BluetoothResult<()> {
            // Simulate connection delay
            std::thread::sleep(std::time::Duration::from_millis(500));

            let device_name = name.map(|s| s.to_string()).unwrap_or_else(|| {
                match address {
                    "00:11:22:33:44:55" => "WH-1000XM6 (Simulated)".to_string(),
                    "AA:BB:CC:DD:EE:FF" => "WH-1000XM5 (Simulated)".to_string(),
                    "11:22:33:44:55:66" => "WH-1000XM4 (Simulated)".to_string(),
                    _ => "Sony Headphones (Simulated)".to_string(),
                }
            });

            self.connected = true;
            self.device = Some(Device {
                name: device_name.clone(),
                address: address.to_string(),
            });
            tracing::info!("Mock: Connected to {}", device_name);
            Ok(())
        }

        fn disconnect(&mut self) -> BluetoothResult<()> {
            self.connected = false;
            self.device = None;
            tracing::info!("Mock: Disconnected");
            Ok(())
        }

        fn send(&mut self, data: &[u8]) -> BluetoothResult<Option<u8>> {
            if !self.connected {
                return Err(BluetoothError::NotConnected);
            }
            tracing::debug!("Mock: Sent {} bytes", data.len());
            // Simulate sequence number alternating (like real device)
            self.seq_number = if self.seq_number == 0 { 1 } else { 0 };
            Ok(Some(self.seq_number))
        }

        fn receive(&mut self) -> BluetoothResult<Vec<u8>> {
            if !self.connected {
                return Err(BluetoothError::NotConnected);
            }
            // Simulate ACK response
            self.seq_number = self.seq_number.wrapping_add(1);
            let ack = protocol::package_for_bluetooth(&[], DataType::Ack, self.seq_number)
                .map_err(|e| BluetoothError::ReceiveFailed(e.to_string()))?;
            tracing::debug!("Mock: Returning ACK");
            Ok(ack)
        }

        fn is_connected(&self) -> bool {
            self.connected
        }

        fn connected_device(&self) -> Option<&Device> {
            self.device.as_ref()
        }
    }
}
