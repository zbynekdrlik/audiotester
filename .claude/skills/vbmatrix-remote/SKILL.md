---
name: vbmatrix-remote
description: Use when controlling VBMatrix audio routing on iem.lan remotely via VBAN Text API. Applies when simulating audio disconnects, muting routing points, verifying audio paths, or restarting VBMatrix engine for E2E testing.
---

# VBMatrix Remote Control

## Overview

Control VB-Audio Matrix on iem.lan via VBAN Text API over UDP. Used to simulate audio disconnects, verify routing, and restart the audio engine during E2E testing.

## CRITICAL SAFETY WARNING

**iem.lan is a PRODUCTION MACHINE used for LIVE EVENTS.**

You are ONLY allowed to modify these specific channels:

| Allowed Device | Allowed Channels | Purpose                        |
| -------------- | ---------------- | ------------------------------ |
| `VASIO8`       | ALL (0-7)        | AudioTester development device |
| `ASIO128`      | 126, 127 ONLY    | Dante loopback for AudioTester |

**FORBIDDEN:**

- Modifying ANY ASIO128 channel 0-125 (production audio)
- Modifying ANY other VBMatrix slots
- Running `Command.Shutdown` (kills VBMatrix)
- Running `Command.Reset` (resets all routing)
- Creating/removing points on production channels

**Before ANY write command, verify:**

1. Target channel is 126 or 127 on ASIO128
2. Or target is VASIO8 device
3. Command is Mute toggle or dBGain (not Remove/Paste)

**If unsure, QUERY FIRST with `=?;` syntax. Never blindly modify.**

## iem.lan Setup

| Component        | Value                   |
| ---------------- | ----------------------- |
| Host             | `iem.lan` (10.77.9.231) |
| User (Windows)   | `ableton-iem\iem`       |
| VBMatrix Version | 1.0.2.5 64-bit          |
| VBAN Port        | 6980 (UDP)              |
| Stream Name      | `Command1`              |

### Slots Configured

| SUID      | Device                   | Purpose                                |
| --------- | ------------------------ | -------------------------------------- |
| `VASIO8`  | VASIO-8 virtual ASIO     | AudioTester uses this device           |
| `ASIO128` | AIC128-D ASIO Driver x64 | Dante network interface (128 channels) |

### Audio Loop for Testing

```
AudioTester -> VASIO8.OUT[1,2] -> VBMatrix -> ASIO128.OUT[126,127]
    -> Dante (loopback in Dante Controller)
    -> ASIO128.IN[126,127] -> VBMatrix -> VASIO8.IN[1,2] -> AudioTester
```

**Key routing points for testing:**

- Send: `VASIO8.IN[1] -> ASIO128.OUT[126]`
- Return: `ASIO128.IN[126] -> VASIO8.OUT[1]`

## Quick Reference

### VBAN Text Packet Format

```python
header = b'VBAN' + bytes([0x52, 0x00, 0x00, 0x10]) + b'Command1\x00' * padded_to_16 + frame_counter_4bytes
packet = header + command.encode('utf-8')
# Send via UDP to iem.lan:6980
```

### Command Syntax

```
Point(SUID.IN[n],SUID.OUT[m]).Property=value;   # Set value (no spaces!)
Point(SUID.IN[n],SUID.OUT[m]).Property=?;       # Query value
Command.Property=value;                          # System command
Slot(SUID).Property=value;                       # Slot control
```

### Common Operations

| Operation             | Command                                             |
| --------------------- | --------------------------------------------------- |
| **Mute point**        | `Point(VASIO8.IN[1],ASIO128.OUT[126]).Mute=1;`      |
| **Unmute point**      | `Point(VASIO8.IN[1],ASIO128.OUT[126]).Mute=0;`      |
| **Query mute**        | `Point(VASIO8.IN[1],ASIO128.OUT[126]).Mute=?;`      |
| **Set gain**          | `Point(VASIO8.IN[1],ASIO128.OUT[126]).dBGain=-6.0;` |
| **Query gain**        | `Point(VASIO8.IN[1],ASIO128.OUT[126]).dBGain=?;`    |
| **Restart engine**    | `Command.Restart=1;`                                |
| **Get version**       | `Command.Version=?;`                                |
| **Get matrix state**  | `Command.Matrix=?;`                                 |
| **Check slot online** | `Slot(VASIO8).Online=?;`                            |

### Simulate Disconnect/Reconnect

```bash
# Disconnect return path (simulates cable unplug)
vban_cmd "Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=1;"

# Reconnect
vban_cmd "Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=0;"
```

## Python Implementation

```python
#!/usr/bin/env python3
"""VBAN Text command utility for VBMatrix on iem.lan - WITH SAFETY CHECKS"""
import socket
import struct
import re

# SAFETY: Only these channels are allowed to be modified
ALLOWED_ASIO128_CHANNELS = {126, 127}
FORBIDDEN_COMMANDS = {'Command.Shutdown', 'Command.Reset'}

def validate_command(command: str) -> bool:
    """Validate command doesn't touch production channels."""
    # Always allow queries
    if "=?" in command:
        return True

    # Block dangerous commands
    for forbidden in FORBIDDEN_COMMANDS:
        if forbidden in command:
            raise ValueError(f"FORBIDDEN: {forbidden} would disrupt production")

    # Check Point commands for ASIO128 channels
    match = re.search(r'Point\(ASIO128\.(?:IN|OUT)\[(\d+)\]', command)
    if match:
        channel = int(match.group(1))
        if channel not in ALLOWED_ASIO128_CHANNELS:
            raise ValueError(f"FORBIDDEN: ASIO128 channel {channel} is production (only 126,127 allowed)")

    return True

def send_vbmatrix_command(command: str, host: str = "iem.lan", port: int = 6980) -> str:
    """Send command to VBMatrix and return response."""
    # SAFETY CHECK
    validate_command(command)

    # Build VBAN-TEXT header
    header = b'VBAN'
    header += bytes([0x52, 0x00, 0x00, 0x10])  # TEXT subprotocol, UTF8
    header += b'Command1\x00\x00\x00\x00\x00\x00\x00\x00'  # Stream name (16 bytes)
    header += struct.pack('<I', 0)  # Frame counter

    sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
    sock.settimeout(2.0)

    try:
        sock.sendto(header + command.encode('utf-8'), (host, port))
        if "=?" in command:  # Query - wait for response
            data, _ = sock.recvfrom(1500)
            return data[28:].decode('utf-8').strip()
        return "OK"
    except socket.timeout:
        return "TIMEOUT"
    finally:
        sock.close()

# Usage examples:
# send_vbmatrix_command("Command.Version=?;")
# send_vbmatrix_command("Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=1;")  # OK - channel 126
# send_vbmatrix_command("Point(ASIO128.IN[5],VASIO8.OUT[1]).Mute=1;")    # BLOCKED - production
```

## Shell Script (via SSH)

```bash
# Create command helper on local machine
vban_cmd() {
    python3 -c "
import socket, struct
h = b'VBAN' + bytes([0x52,0,0,0x10]) + b'Command1'.ljust(16,b'\x00') + struct.pack('<I',0)
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
s.settimeout(2)
s.sendto(h + '$1'.encode(), ('iem.lan', 6980))
try:
    print(s.recv(1500)[28:].decode())
except: pass
"
}

# Disconnect audio loop
vban_cmd 'Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=1;'

# Reconnect
vban_cmd 'Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=0;'
```

## E2E Testing Patterns

### MANDATORY: Always Restore State

**Every test that modifies VBMatrix MUST restore original state, even on failure.**

```python
import contextlib

@contextlib.contextmanager
def vbmatrix_disconnect_test():
    """Context manager that ensures reconnection even on test failure."""
    original_state = send_vbmatrix_command("Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=?;")
    try:
        # Disconnect for testing
        send_vbmatrix_command("Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=1;")
        yield
    finally:
        # ALWAYS reconnect, even if test fails
        send_vbmatrix_command("Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=0;")
        # Verify restoration
        restored = send_vbmatrix_command("Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=?;")
        if "Mute = 0" not in restored:
            raise RuntimeError("CRITICAL: Failed to restore VBMatrix state!")

# Usage:
with vbmatrix_disconnect_test():
    # Test code here - state will be restored even on exception
    pass
```

### Test Audio Loss Detection

```python
# 1. Verify loop is connected
assert "Mute = 0" in send_vbmatrix_command("Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=?;")

# 2. Start monitoring, verify signal
# ... audiotester API calls ...

# 3. Disconnect return path
send_vbmatrix_command("Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=1;")

# 4. Verify audiotester detects loss
# ... check stats API for signal_detected=false ...

# 5. ALWAYS reconnect (wrap in try/finally in real code!)
send_vbmatrix_command("Point(ASIO128.IN[126],VASIO8.OUT[1]).Mute=0;")

# 6. Verify signal recovers
```

### Test Engine Restart Recovery

```python
# Restart VBMatrix engine
send_vbmatrix_command("Command.Restart=1;")
time.sleep(3)  # Engine restart takes ~2-3 seconds

# Verify engine is back
assert "ASIO128" in send_vbmatrix_command("Command.Matrix=?;")
```

## Error Responses

| Response              | Meaning                                   |
| --------------------- | ----------------------------------------- |
| `Property = value;`   | Success (query)                           |
| `Property = Err;`     | Point doesn't exist or invalid syntax     |
| `(no response)`       | Set command succeeded (no reply for sets) |
| `TIMEOUT`             | VBMatrix not responding or wrong port     |
| `Slot(X).Online = -;` | Slot exists but device offline            |

## Common Mistakes

| Mistake                             | Fix                                                                         |
| ----------------------------------- | --------------------------------------------------------------------------- |
| Using spaces in Point syntax        | Remove all spaces: `Point(A.IN[0],B.OUT[0])` not `Point(A.IN[0], B.OUT[0])` |
| Wrong channel index                 | Channels are 0-indexed in API but 1-indexed in UI                           |
| Querying non-existent point         | "Err" response means point not routed in VBMatrix                           |
| Expecting response for set commands | Set commands don't return responses                                         |
| Wrong SUID case                     | SUIDs are case-sensitive: `VASIO8` not `Vasio8`                             |

## Verified Routing Points (iem.lan)

### VASIO8 -> ASIO128 (Send to Dante)

All VASIO8 inputs 1-7 route to ASIO128 outputs 126-127:

- `VASIO8.IN[1-7] -> ASIO128.OUT[126]`
- `VASIO8.IN[1-7] -> ASIO128.OUT[127]`

### ASIO128 -> VASIO8 (Return from Dante)

ASIO128 inputs 126-127 route to VASIO8 outputs 1-7:

- `ASIO128.IN[126] -> VASIO8.OUT[1-7]`
- `ASIO128.IN[127] -> VASIO8.OUT[1-7]`

### Internal ASIO128 Loopback

Channels 1-7 looped internally (for testing without Dante):

- `ASIO128.IN[1-7] -> ASIO128.OUT[1-7]`
