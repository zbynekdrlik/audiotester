# Audiotester

Windows ASIO audio testing application for monitoring professional audio paths (Dante, VBAN, VBMatrix).

## Features

- **ASIO Integration**: Low-latency audio I/O with professional audio interfaces
- **Latency Measurement**: Sample-accurate latency detection using MLS correlation
- **Sample Loss Detection**: Tracks lost and corrupted samples in real-time
- **System Tray**: Unobtrusive monitoring with color-coded status indication
- **Statistics Dashboard**: Historical graphs for latency and loss metrics

## Installation

### Quick Install (PowerShell)

```powershell
irm https://raw.githubusercontent.com/newlevel/audiotester/main/installer/install.ps1 | iex
```

### Manual Installation

1. Download the latest release from [Releases](https://github.com/newlevel/audiotester/releases)
2. Extract to your preferred location
3. Run `audiotester.exe`

## Building from Source

### Prerequisites

- Windows 10/11
- Rust toolchain (MSVC)
- LLVM/Clang
- ASIO SDK (download from Steinberg)

### Setup

```powershell
# Install Rust
winget install Rustlang.Rust.MSVC

# Install LLVM
winget install LLVM.LLVM

# Set ASIO SDK path
$env:CPAL_ASIO_DIR = "C:\path\to\asiosdk"

# Build
cargo build --release
```

### Running Tests

```bash
# All tests
cargo test

# E2E tests only
cargo test --test e2e_*
```

## Usage

1. Launch Audiotester
2. Right-click the tray icon to select your ASIO device
3. Configure your audio routing to loop channel 1 output back to channel 1 input
4. The tray icon indicates status:
   - **Green**: Audio path healthy
   - **Orange**: High latency or minor issues
   - **Red**: Lost samples or signal problems
   - **Gray**: Not monitoring

5. Click "Show Statistics" for detailed metrics and graphs

## Architecture

```
┌─────────────────────────────────────────────────────────┐
│                    System Tray (tray-icon)              │
│  Status: Green/Orange/Red  │  Menu: Device, Stats, Exit │
└─────────────────────────────┬───────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────┐
│                Statistics Window (egui)                  │
│  [Latency Graph] [Sample Loss Graph] [Device Info]      │
└─────────────────────────────┬───────────────────────────┘
                              │
┌─────────────────────────────▼───────────────────────────┐
│                  Audio Engine (cpal/ASIO)                │
│  MLS Generator → Ring Buffer → Analyzer                  │
│       ↓              ↑            ↓                      │
│  ASIO Output → [External Loop] → ASIO Input              │
└─────────────────────────────────────────────────────────┘
```

## How It Works

Audiotester uses a **Maximum Length Sequence (MLS)** as the test signal:

1. **Generation**: A pseudo-random binary sequence with perfect autocorrelation properties
2. **Transmission**: Signal sent on output channel 1
3. **Reception**: Looped signal received on input channel 1
4. **Correlation**: FFT-based cross-correlation finds the delay
5. **Analysis**: Latency calculated from peak position; loss detected from sequence gaps

MLS properties:

- Length: 32767 samples (~0.68s at 48kHz)
- Each sample position is uniquely identifiable
- Robust detection even with noise and distortion

## Configuration

Audiotester saves settings to `%APPDATA%\Audiotester\config.json`:

```json
{
  "device": "Your ASIO Device",
  "sample_rate": 48000,
  "latency_warning_ms": 10.0,
  "latency_error_ms": 20.0
}
```

## Contributing

1. Fork the repository
2. Create a feature branch from `dev`
3. Write tests (E2E tests required for new features)
4. Ensure `cargo fmt` and `cargo clippy` pass
5. Open a PR to `dev`

## License

MIT License - see [LICENSE](LICENSE) for details.
