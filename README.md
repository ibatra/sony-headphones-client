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
- System tray integration
- Cross-platform (Windows, Linux, macOS)

## Platform Status

| Platform | Status |
|----------|--------|
| Windows  | Working (tested) |
| Linux    | Implemented (untested) |
| macOS    | Stub only |

## Tech Stack

- **Frontend**: Svelte + TypeScript
- **Backend**: Rust + Tauri v2
- **Bluetooth**: RFCOMM (platform-specific implementations)

## Building

```bash
# Install dependencies
bun install

# Development
bun run tauri dev

# Build
bun run tauri build
```

## Protocol

This app communicates with Sony headphones using the same Bluetooth RFCOMM protocol as the official Sony Headphones Connect app. The XM6 protocol was reverse-engineered from Bluetooth HCI traffic captures.

The protocol uses a bidirectional ACK handshake:
1. Client sends command with sequence number
2. Device responds with ACK containing next sequence number
3. Client sends ACK back to device

References:
- [mos9527/SonyHeadphonesClient](https://github.com/mos9527/SonyHeadphonesClient) - Protocol reference

## License

MIT
