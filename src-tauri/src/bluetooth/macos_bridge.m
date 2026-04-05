// macOS Bluetooth RFCOMM bridge for Sony Headphones Client
//
// This Objective-C file wraps Apple's IOBluetooth framework into a C API
// that can be called from Rust via FFI. It handles:
// - Device discovery (paired Bluetooth devices)
// - RFCOMM channel connection with SDP service lookup
// - Synchronous send and receive with delegate-based data buffering
// Compiled with Manual Reference Counting (-fno-objc-arc).
// Linked against IOBluetooth.framework and Foundation.framework.

#import <IOBluetooth/IOBluetooth.h>
#import <Foundation/Foundation.h>
#import <AppKit/NSApplication.h>
#include <string.h>
#include <dispatch/dispatch.h>

// ============================================================================
// Data structures exposed to Rust via FFI
// ============================================================================

typedef struct {
    char name[256];
    char address[18]; // "XX:XX:XX:XX:XX:XX\0"
} SonyBTDeviceInfo;

// ============================================================================
// Connection class - acts as RFCOMM channel delegate
// ============================================================================

@interface SonyBTConnection : NSObject <IOBluetoothRFCOMMChannelDelegate>
@property (assign) IOBluetoothDevice *device;
@property (assign) IOBluetoothRFCOMMChannel *rfcommChannel;
@property (assign) NSMutableData *receiveBuffer;
@property (assign) NSCondition *bufferCondition;
@property (assign) BOOL connected;
@end

@implementation SonyBTConnection

- (instancetype)init {
    self = [super init];
    if (self) {
        _receiveBuffer = [[NSMutableData alloc] init];
        _bufferCondition = [[NSCondition alloc] init];
        _connected = NO;
    }
    return self;
}

- (void)dealloc {
    [_receiveBuffer release];
    [_bufferCondition release];
    if (_rfcommChannel) [_rfcommChannel release];
    if (_device) [_device release];
    [super dealloc];
}

// IOBluetoothRFCOMMChannelDelegate - called on the run loop thread that opened the channel
- (void)rfcommChannelData:(IOBluetoothRFCOMMChannel *)rfcommChannel
                     data:(void *)dataPointer
                   length:(size_t)dataLength {
    [_bufferCondition lock];
    [_receiveBuffer appendBytes:dataPointer length:dataLength];
    [_bufferCondition signal];
    [_bufferCondition unlock];
}

- (void)rfcommChannelClosed:(IOBluetoothRFCOMMChannel *)rfcommChannel {
    NSLog(@"[SonyBT] RFCOMM channel closed");
    _connected = NO;
    [_bufferCondition lock];
    [_bufferCondition signal]; // Wake up any waiting receiver
    [_bufferCondition unlock];
}

- (void)rfcommChannelOpenComplete:(IOBluetoothRFCOMMChannel *)rfcommChannel
                           status:(IOReturn)error {
    if (error == kIOReturnSuccess) {
        NSLog(@"[SonyBT] RFCOMM channel opened successfully");
    } else {
        NSLog(@"[SonyBT] RFCOMM channel open failed: 0x%08X", error);
        _connected = NO;
    }
}

- (void)disconnect {
    if (_rfcommChannel) {
        [_rfcommChannel setDelegate:nil];
        [_rfcommChannel closeChannel];
        [_rfcommChannel release];
        _rfcommChannel = nil;
    }
    if (_device) {
        [_device closeConnection];
        [_device release];
        _device = nil;
    }
    _connected = NO;
}

@end

// ============================================================================
// C API - called from Rust via FFI
// ============================================================================

/// Hide the app from the Dock and Cmd-Tab switcher.
/// Must be called early, before any windows are created.
void sony_bt_hide_from_dock(void) {
    dispatch_async(dispatch_get_main_queue(), ^{
        [NSApp setActivationPolicy:NSApplicationActivationPolicyAccessory];
        NSLog(@"[SonyBT] Hidden from Dock");
    });
}

/// Discover all paired Bluetooth devices.
/// Returns the number of devices written to out_devices (up to max_count).
/// The Rust side handles filtering for Sony headphones.
int sony_bt_discover(SonyBTDeviceInfo *out_devices, int max_count) {
    @autoreleasepool {
        NSArray *paired = [IOBluetoothDevice pairedDevices];
        if (!paired) {
            NSLog(@"[SonyBT] No paired devices found (pairedDevices returned nil)");
            return 0;
        }

        int count = 0;
        for (IOBluetoothDevice *dev in paired) {
            if (count >= max_count) break;

            NSString *name = [dev name];
            NSString *addr = [dev addressString];
            if (!name || !addr) continue;

            strncpy(out_devices[count].name, [name UTF8String], 255);
            out_devices[count].name[255] = '\0';

            // IOBluetooth returns addresses as "xx-xx-xx-xx-xx-xx"
            // Convert to "XX:XX:XX:XX:XX:XX" for consistency with other platforms
            NSString *formatted = [[addr uppercaseString]
                stringByReplacingOccurrencesOfString:@"-" withString:@":"];
            strncpy(out_devices[count].address, [formatted UTF8String], 17);
            out_devices[count].address[17] = '\0';

            count++;
        }

        NSLog(@"[SonyBT] Discovery: found %d paired devices", count);
        return count;
    }
}

/// Connect to a Bluetooth device by MAC address and open an RFCOMM channel.
/// Returns an opaque connection handle, or NULL on failure.
/// out_error: 0 = success, 1 = device not found, 2 = connection failed
///
/// The connection is established on the main thread to ensure IOBluetooth
/// delegate callbacks fire on the main run loop (required by IOBluetooth).
void *sony_bt_connect(const char *address, int *out_error) {
    __block void *result = NULL;
    __block int error = 0;

    void (^connectBlock)(void) = ^{
        @autoreleasepool {
            // Convert "XX:XX:XX:XX:XX:XX" to "xx-xx-xx-xx-xx-xx" (IOBluetooth format)
            NSString *addrStr = [NSString stringWithUTF8String:address];
            NSString *dashAddr = [[addrStr lowercaseString]
                stringByReplacingOccurrencesOfString:@":" withString:@"-"];

            NSLog(@"[SonyBT] Connecting to %@", dashAddr);

            IOBluetoothDevice *device = [IOBluetoothDevice deviceWithAddressString:dashAddr];
            if (!device) {
                NSLog(@"[SonyBT] Device not found: %@", dashAddr);
                error = 1;
                return;
            }

            SonyBTConnection *conn = [[SonyBTConnection alloc] init];
            conn.device = [device retain];

            // Try SDP lookup with Sony's proprietary service UUID
            // 96CC203E-5068-46ad-B32D-E316F5E069BA
            uint8_t sonyUUIDBytes[] = {
                0x96, 0xCC, 0x20, 0x3E,
                0x50, 0x68,
                0x46, 0xAD,
                0xB3, 0x2D,
                0xE3, 0x16, 0xF5, 0xE0, 0x69, 0xBA
            };
            IOBluetoothSDPUUID *sonyUUID =
                [IOBluetoothSDPUUID uuidWithBytes:sonyUUIDBytes length:16];

            BluetoothRFCOMMChannelID channelID = 0;

            // First try: Sony proprietary UUID
            IOBluetoothSDPServiceRecord *serviceRecord =
                [device getServiceRecordForUUID:sonyUUID];
            if (serviceRecord) {
                [serviceRecord getRFCOMMChannelID:&channelID];
                NSLog(@"[SonyBT] Found Sony service on RFCOMM channel %d", channelID);
            }

            // Second try: Serial Port Profile UUID (0x1101)
            if (channelID == 0) {
                IOBluetoothSDPUUID *sppUUID = [IOBluetoothSDPUUID uuid16:0x1101];
                serviceRecord = [device getServiceRecordForUUID:sppUUID];
                if (serviceRecord) {
                    [serviceRecord getRFCOMMChannelID:&channelID];
                    NSLog(@"[SonyBT] Found SPP service on RFCOMM channel %d", channelID);
                }
            }

            // Try RFCOMM channels. Channel 9 is the known stable channel for
            // Sony headphones on macOS. The SPP/SDP channel (varies: 23, 26, etc.)
            // connects but the headphones close it after ~17 seconds.
            // So try channel 9 first, then SDP channel, then other fallbacks.
            IOBluetoothRFCOMMChannel *channel = nil;
            IOReturn status = kIOReturnError;

            // Priority order: 9 first (stable), then SDP channel, then 5, 1
            BluetoothRFCOMMChannelID tryChannels[5];
            int tryCount = 0;

            tryChannels[tryCount++] = 9; // Known stable for Sony
            if (channelID != 0 && channelID != 9) {
                tryChannels[tryCount++] = channelID; // SDP result
            }
            tryChannels[tryCount++] = 5;
            tryChannels[tryCount++] = 1;

            for (int i = 0; i < tryCount; i++) {
                channel = nil;
                NSLog(@"[SonyBT] Trying RFCOMM channel %d", tryChannels[i]);
                status = [device openRFCOMMChannelSync:&channel
                                        withChannelID:tryChannels[i]
                                             delegate:conn];
                if (status == kIOReturnSuccess && channel) {
                    NSLog(@"[SonyBT] Connected on RFCOMM channel %d", tryChannels[i]);
                    break;
                }
            }

            if (status != kIOReturnSuccess || !channel) {
                NSLog(@"[SonyBT] Failed to open RFCOMM channel (status: 0x%08X)", status);
                [conn release];
                error = 2;
                return;
            }

            conn.rfcommChannel = [channel retain];
            conn.connected = YES;

            NSLog(@"[SonyBT] Connection established to %@", [device name]);
            result = (void *)conn; // Caller takes ownership (via alloc)
            error = 0;
        }
    };

    // IOBluetooth delegate callbacks require a run loop. Dispatch to main thread
    // so callbacks fire on the main run loop (which runs in Tauri's event loop).
    if ([NSThread isMainThread]) {
        connectBlock();
    } else {
        dispatch_sync(dispatch_get_main_queue(), connectBlock);
    }

    *out_error = error;
    return result;
}

/// Send data over the RFCOMM channel.
/// Returns bytes sent on success, -1 on failure.
/// Dispatched to main thread for IOBluetooth thread safety.
int sony_bt_send(void *connection, const uint8_t *data, int length) {
    SonyBTConnection *conn = (SonyBTConnection *)connection;
    if (!conn || !conn.connected || !conn.rfcommChannel) return -1;

    __block int sendResult = -1;

    void (^sendBlock)(void) = ^{
        @autoreleasepool {
            IOReturn status = [conn.rfcommChannel writeSync:(void *)data
                                                     length:(UInt16)length];
            if (status == kIOReturnSuccess) {
                sendResult = length;
            } else {
                NSLog(@"[SonyBT] writeSync failed: 0x%08X", status);
                sendResult = -1;
            }
        }
    };

    // writeSync should be called from the same thread that opened the channel
    if ([NSThread isMainThread]) {
        sendBlock();
    } else {
        dispatch_sync(dispatch_get_main_queue(), sendBlock);
    }

    return sendResult;
}

/// Receive data from the RFCOMM channel.
/// Blocks until data is available or timeout_ms expires.
/// Returns bytes read, 0 on timeout, -1 on error/disconnected.
int sony_bt_receive(void *connection, uint8_t *buffer, int buffer_size, int timeout_ms) {
    SonyBTConnection *conn = (SonyBTConnection *)connection;
    if (!conn || !conn.connected) return -1;

    [conn.bufferCondition lock];

    // Wait for data if buffer is empty
    if ([conn.receiveBuffer length] == 0) {
        NSDate *timeout = [NSDate dateWithTimeIntervalSinceNow:timeout_ms / 1000.0];
        [conn.bufferCondition waitUntilDate:timeout];
    }

    // Check for disconnection during wait
    if (!conn.connected) {
        [conn.bufferCondition unlock];
        return -1;
    }

    NSUInteger available = [conn.receiveBuffer length];
    if (available == 0) {
        [conn.bufferCondition unlock];
        return 0; // Timeout
    }

    NSUInteger toCopy = available < (NSUInteger)buffer_size ? available : (NSUInteger)buffer_size;
    memcpy(buffer, [conn.receiveBuffer bytes], toCopy);

    // Remove consumed bytes from the front of the buffer
    [conn.receiveBuffer replaceBytesInRange:NSMakeRange(0, toCopy)
                                  withBytes:NULL
                                     length:0];

    [conn.bufferCondition unlock];
    return (int)toCopy;
}

/// Disconnect and free the connection.
void sony_bt_disconnect(void *connection) {
    SonyBTConnection *conn = (SonyBTConnection *)connection;
    if (!conn) return;

    NSLog(@"[SonyBT] Disconnecting");

    void (^disconnectBlock)(void) = ^{
        @autoreleasepool {
            [conn disconnect];
        }
    };

    if ([NSThread isMainThread]) {
        disconnectBlock();
    } else {
        dispatch_sync(dispatch_get_main_queue(), disconnectBlock);
    }

    [conn release];
}

/// Check if the connection is still active.
int sony_bt_is_connected(void *connection) {
    SonyBTConnection *conn = (SonyBTConnection *)connection;
    return (conn && conn.connected) ? 1 : 0;
}
