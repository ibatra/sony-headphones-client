# Sony Headphones Client

> **WIP** - Work in Progress

Cross-platform desktop app for controlling Sony WH/WF-1000XM series headphones via Bluetooth.

## Supported Devices

- WH-1000XM6 (primary focus)
- WH-1000XM5
- WH-1000XM4
- WH-1000XM3
- WF-1000XM5
- WF-1000XM4
- WF-1000XM3

## Features

- Noise Cancelling / Ambient Sound Mode control
- Equalizer presets
- Volume control
- Battery status
- DSEE (audio upsampling) toggle
- Speak-to-Chat settings (XM5/XM6)
- System tray / menu bar integration
- Cross-platform (Windows, Linux, macOS)

## Platform Status

| Platform | Status | Notes |
|----------|--------|-------|
| Windows  | Working (tested) | Full GUI control via Winsock2 RFCOMM |
| Linux    | Implemented (untested) | BlueZ D-Bus + RFCOMM sockets |
| macOS    | In progress | See [macOS Status](#macos-status) below |

## macOS Status

macOS support is **under active development**. Here is the current state:

### Working

- Headphone physical button gestures (volume, ANC toggle, track skip) work correctly via macOS AVRCP
- Bluetooth RFCOMM connection to WH-1000XM6 establishes successfully (auto-discovery, auto-connect on channel 9)
- App runs as a background **menu bar agent** (no dock icon, lives in the system tray)
- Auto-start at login via LaunchAgent
- IOBluetooth Objective-C bridge for device discovery, RFCOMM channel management, and delegate-based data buffering
- Protocol framing, ACK handling, and stale notification draining

### Not Working (Under Development)

- **GUI controls do not change headphone settings** — ANC mode buttons, EQ presets, volume slider, playback controls, DSEE toggle, and speak-to-chat settings are visible in the UI but do not affect the headphones
- **Menu bar tray ANC shortcuts** do not change headphone settings
- RFCOMM commands are sent to the headphones and ACKed (the device acknowledges receipt), but the headphones do not execute them. This suggests either the wrong RFCOMM channel, a missing initialization handshake, or incorrect command format for the XM6 on macOS
- Volume and ANC state from headset gestures are not reflected in the GUI

### Known Issues

- The app opens an RFCOMM connection on startup which may briefly interrupt the Bluetooth audio connection while the channel is being established
- Battery queries are ACKed but do not return battery data (falls back to simulated values)

## Tech Stack

- **Frontend**: Svelte 5 + TypeScript + Tailwind CSS
- **Backend**: Rust + Tauri v2
- **Bluetooth**: RFCOMM via platform-specific implementations
  - Windows: Winsock2 BTH sockets
  - Linux: BlueZ D-Bus + libc RFCOMM sockets
  - macOS: IOBluetooth framework via Objective-C bridge (compiled with `cc` crate)

## Installation

### Prerequisites

- [Bun](https://bun.sh/) (package manager)
- [Rust](https://rustup.rs/) (1.70+)
- Platform-specific:
  - **Windows**: No extra dependencies
  - **Linux**: `libbluetooth-dev` and `libdbus-1-dev`
  - **macOS**: Xcode Command Line Tools (`xcode-select --install`)

### Build from Source

```bash
# Clone the repo
git clone https://github.com/fcampoverdeg/sony-headphones-client.git
cd sony-headphones-client

# Install frontend dependencies
bun install

# Development mode (hot reload)
bun tauri dev

# Production build
bun tauri build
```

### macOS Installation

After building, the app bundle is at:
```
src-tauri/target/release/bundle/macos/Sony Headphones.app
```

1. **Copy to Applications:**
   ```bash
   cp -R "src-tauri/target/release/bundle/macos/Sony Headphones.app" /Applications/
   ```

2. **Launch:**
   ```bash
   open "/Applications/Sony Headphones.app"
   ```
   The app runs as a menu bar agent (no dock icon). Look for the headphones icon in your menu bar.

3. **(Optional) Auto-start at login:**
   ```bash
   cp com.ishaan.sony-headphones-client.plist ~/Library/LaunchAgents/
   ```

4. **Make sure your headphones are paired** with your Mac in System Settings > Bluetooth before launching the app.

## Protocol

This app communicates with Sony headphones using the same Bluetooth RFCOMM protocol as the official Sony Headphones Connect app. The XM6 protocol was reverse-engineered from Bluetooth HCI traffic captures.

The protocol uses a bidirectional ACK handshake:
1. Client sends command with sequence number (alternating 0/1)
2. Device responds with ACK containing next sequence number
3. Client sends ACK back to device

References:
- [mos9527/SonyHeadphonesClient](https://github.com/mos9527/SonyHeadphonesClient) - Protocol reference

## Contributing

macOS support needs help! If you have experience with:
- IOBluetooth RFCOMM protocol debugging
- Sony headphones Bluetooth HCI traffic analysis
- Tauri v2 on macOS

Please open an issue or PR.

## License

MIT
