#!/bin/bash
# Install Sony Headphones Background Service
# Builds the app, extracts the binary, and registers it as a LaunchAgent
# that starts automatically on login. Runs as a pure background process
# with no Dock icon, no menu bar icon, no windows.

set -e

INSTALL_DIR="$HOME/Library/Sony Headphones"
PLIST_NAME="com.ishaan.sony-headphones-client"
LAUNCH_AGENTS="$HOME/Library/LaunchAgents"

echo "=== Sony Headphones Background Service Installer ==="
echo ""

# Step 1: Build the release binary
echo "[1/4] Building release binary..."
cargo tauri build 2>&1 | tail -5

BUILT_BINARY="src-tauri/target/release/sony-headphones-client"
if [ ! -f "$BUILT_BINARY" ]; then
    echo "ERROR: Build failed — binary not found"
    exit 1
fi

# Step 2: Stop existing service if running
echo "[2/4] Stopping existing service (if running)..."
launchctl bootout gui/$(id -u) "$LAUNCH_AGENTS/$PLIST_NAME.plist" 2>/dev/null || true
pkill -9 -f "sony-headphones-client" 2>/dev/null || true
sleep 1

# Step 3: Install the binary (not the .app bundle — avoids Dock icon)
echo "[3/4] Installing binary..."
mkdir -p "$INSTALL_DIR"
cp "$BUILT_BINARY" "$INSTALL_DIR/sony-headphones-client"
chmod +x "$INSTALL_DIR/sony-headphones-client"
echo "    Installed: $INSTALL_DIR/sony-headphones-client"

# Step 4: Install and start the LaunchAgent
echo "[4/4] Installing LaunchAgent (auto-start on login)..."

# Generate plist with correct home directory
mkdir -p "$LAUNCH_AGENTS"
cat > "$LAUNCH_AGENTS/$PLIST_NAME.plist" << EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>Label</key>
	<string>$PLIST_NAME</string>
	<key>ProgramArguments</key>
	<array>
		<string>$INSTALL_DIR/sony-headphones-client</string>
	</array>
	<key>RunAtLoad</key>
	<true/>
	<key>KeepAlive</key>
	<dict>
		<key>SuccessfulExit</key>
		<false/>
	</dict>
	<key>ProcessType</key>
	<string>Background</string>
	<key>LimitLoadToSessionType</key>
	<string>Aqua</string>
	<key>StandardOutPath</key>
	<string>/tmp/sony-headphones.log</string>
	<key>StandardErrorPath</key>
	<string>/tmp/sony-headphones.log</string>
</dict>
</plist>
EOF

launchctl bootstrap gui/$(id -u) "$LAUNCH_AGENTS/$PLIST_NAME.plist"

echo ""
echo "=== Done! ==="
echo "The service is now running and will start automatically on login."
echo "No Dock icon, no menu bar icon — completely invisible."
echo ""
echo "Useful commands:"
echo "  View logs:      tail -f /tmp/sony-headphones.log"
echo "  Stop service:   launchctl bootout gui/\$(id -u) ~/Library/LaunchAgents/$PLIST_NAME.plist"
echo "  Start service:  launchctl bootstrap gui/\$(id -u) ~/Library/LaunchAgents/$PLIST_NAME.plist"
echo "  Uninstall:      bash uninstall.sh"
