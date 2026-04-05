#!/bin/bash
# Uninstall Sony Headphones Background Service

set -e

PLIST_NAME="com.ishaan.sony-headphones-client"

echo "=== Uninstalling Sony Headphones Service ==="

echo "Stopping service..."
launchctl bootout gui/$(id -u) ~/Library/LaunchAgents/$PLIST_NAME.plist 2>/dev/null || true
pkill -9 -f "sony-headphones-client" 2>/dev/null || true

echo "Removing LaunchAgent..."
rm -f ~/Library/LaunchAgents/$PLIST_NAME.plist

echo "Removing binary..."
rm -rf ~/Library/Sony\ Headphones

echo "Removing app bundle (if present)..."
rm -rf "/Applications/Sony Headphones.app"

echo "Cleaning up logs..."
rm -f /tmp/sony-headphones.log

echo ""
echo "Done. Service has been completely removed."
