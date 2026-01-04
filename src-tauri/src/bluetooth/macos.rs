//! macOS Bluetooth implementation using IOBluetooth
//!
//! This module provides Bluetooth RFCOMM connectivity on macOS using:
//! - IOBluetooth framework for device discovery and RFCOMM

use super::{BluetoothConnector, BluetoothError, BluetoothResult, Device};

/// macOS Bluetooth connector using IOBluetooth
pub struct MacOSBluetoothConnector {
    connected_device: Option<Device>,
    // TODO: Add IOBluetoothRFCOMMChannel handle
}

impl MacOSBluetoothConnector {
    /// Create a new macOS Bluetooth connector
    pub fn new() -> BluetoothResult<Self> {
        Ok(Self {
            connected_device: None,
        })
    }
}

impl BluetoothConnector for MacOSBluetoothConnector {
    fn discover_devices(&self) -> BluetoothResult<Vec<Device>> {
        // TODO: Implement macOS Bluetooth device discovery
        // Use IOBluetoothDevice and IOBluetoothDeviceInquiry
        Err(BluetoothError::PlatformNotSupported)
    }

    fn connect(&mut self, address: &str) -> BluetoothResult<()> {
        // TODO: Implement macOS RFCOMM connection
        // Use IOBluetoothRFCOMMChannel
        Err(BluetoothError::PlatformNotSupported)
    }

    fn disconnect(&mut self) -> BluetoothResult<()> {
        self.connected_device = None;
        Ok(())
    }

    fn send(&mut self, _data: &[u8]) -> BluetoothResult<()> {
        Err(BluetoothError::NotConnected)
    }

    fn receive(&mut self) -> BluetoothResult<Vec<u8>> {
        Err(BluetoothError::NotConnected)
    }

    fn is_connected(&self) -> bool {
        self.connected_device.is_some()
    }

    fn connected_device(&self) -> Option<&Device> {
        self.connected_device.as_ref()
    }
}
