# Audiotester Project Guidelines

## Project Overview

Windows ASIO audio testing application for monitoring professional audio paths (Dante, VBAN, VBMatrix). The application runs as a system tray utility that sends test signals, receives looped audio, and compares them to detect latency issues and sample loss.

## Agent Behavior

- Act as a highly skilled Rust/ASIO developer with expertise in real-time audio processing
- Prioritize E2E tests for every new feature - tests are the highest priority
- Follow strict PR workflow: all changes go through `dev` branch, then PR to `main`
- Use idiomatic Rust patterns and maintain backward compatibility
- Keep solutions simple and focused - avoid over-engineering

## Code Standards

- Run `cargo fmt` before every commit
- Run `cargo clippy -- -D warnings` and fix all warnings
- All public functions must have doc comments with examples where appropriate
- E2E tests required for new features before merging
- Use `thiserror` for custom error types, `anyhow` for application errors
- Prefer `tracing` over `println!` for logging

## Architecture

```
src/
├── main.rs          # Entry point, tray setup, event loop
├── lib.rs           # Library root, exports public API
├── audio/
│   ├── engine.rs    # ASIO device management, stream handling
│   ├── signal.rs    # MLS generation, test signal creation
│   └── analyzer.rs  # Cross-correlation, latency/loss detection
├── ui/
│   ├── tray.rs      # System tray icon, menu, status colors
│   └── stats_window.rs  # Statistics GUI with graphs
└── stats/
    └── store.rs     # Time-series data storage for metrics
```

## Key Dependencies

| Crate                 | Purpose                     |
| --------------------- | --------------------------- |
| `cpal` (ASIO feature) | Audio I/O with ASIO backend |
| `tray-icon`           | System tray icon and menu   |
| `egui`/`eframe`       | Immediate mode GUI          |
| `egui_plot`           | Real-time charts and graphs |
| `ringbuf`             | Lock-free audio buffers     |
| `rustfft`             | FFT-based cross-correlation |

## Build Requirements

### Windows Development

- Visual Studio Build Tools 2019+ with C++ workload
- LLVM/Clang for ASIO SDK compilation
- ASIO SDK (download from Steinberg, set `CPAL_ASIO_DIR` env var)

### Quick Setup

```powershell
# Install Rust
winget install Rustlang.Rust.MSVC

# Install LLVM
winget install LLVM.LLVM

# Set ASIO SDK path (after downloading)
$env:CPAL_ASIO_DIR = "C:\path\to\asiosdk"
```

## Git Workflow

1. Never push directly to `main`
2. Create feature branches from `dev`
3. Open PR from feature branch to `dev`
4. After review, merge to `dev`
5. Periodically, PR from `dev` to `main` (release)

## Testing Strategy

### E2E Tests (Highest Priority)

- `tests/e2e/signal_test.rs` - MLS generation and properties
- `tests/e2e/latency_test.rs` - Latency measurement accuracy
- `tests/e2e/loss_test.rs` - Sample loss detection

### Integration Tests

- `tests/integration/audio_loop.rs` - Full audio path (mocked ASIO)

### Running Tests

```bash
# All tests
cargo test --all-features

# E2E only
cargo test --test e2e_*

# With output
cargo test -- --nocapture
```

## Signal Processing Notes

### MLS (Maximum Length Sequence)

- Length: 2^15 - 1 = 32767 samples (~0.68s at 48kHz)
- Perfect autocorrelation for precise latency detection
- Each sample position is uniquely identifiable

### Latency Calculation

```
latency_samples = peak_position_in_correlation
latency_ms = latency_samples / sample_rate * 1000
```

### Sample Loss Detection

- Track frame markers embedded in MLS
- Compare expected vs received sequence numbers
- Report gaps as lost samples

## Common Tasks

### Adding a New Feature

1. Create feature branch: `git checkout -b feature/name dev`
2. Write E2E tests first
3. Implement feature
4. Run `cargo fmt && cargo clippy -- -D warnings`
5. Run `cargo test`
6. Commit with descriptive message
7. Open PR to `dev`

### Debugging Audio Issues

- Enable trace logging: `RUST_LOG=audiotester=trace cargo run`
- Check ASIO device list: Look for "ASIO" in device names
- Verify sample rate matches between devices

## Performance Targets

- Audio callback: < 1ms processing time
- UI updates: 60 FPS in stats window
- Memory: < 50MB typical usage
- CPU: < 5% during monitoring
