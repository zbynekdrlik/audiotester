# Audiotester Project Guidelines

## Project Overview

Windows ASIO audio testing application for monitoring professional audio paths (Dante, VBAN, VBMatrix). The application runs as a system tray utility that sends test signals, receives looped audio, and compares them to detect latency issues and sample loss.

## Agent Behavior

- Act as a highly skilled Rust/ASIO developer with expertise in real-time audio processing
- Prioritize E2E tests for every new feature - tests are the highest priority
- **NEVER create feature branches** - only `main` and `dev` branches exist
- **ALL work happens directly on `dev` branch** - commit and push to `dev`
- **ALL compilation, testing, and releases happen through GitHub CI/CD** - never compile locally on test machines
- **Test machines are for TESTING ONLY** - never install dev tools or set up dev environments on them
- Use idiomatic Rust patterns and maintain backward compatibility
- Keep solutions simple and focused - avoid over-engineering
- **ALWAYS provide a mergeable PR URL** - After completing work, the agent MUST:
  1. Push changes to `dev` branch
  2. Wait for CI to pass (check GitHub Actions)
  3. Create PR from `dev` to `main`
  4. Provide the PR URL to the user
  5. PR must be GREEN (CI passing) and ready to merge
- **VERSION, RELEASE, AND DEPLOY ARE FULLY AUTOMATIC** - After PR merge to `main`:
  - `auto-release.yml` handles everything: version bump → build → release → deploy
  - Agent does NOT need to manually create tags or trigger workflows
  - Agent should monitor the auto-release workflow and report result to user

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

**CRITICAL: Only two branches are allowed in this repository: `main` and `dev`**

### Branch Policy (STRICTLY ENFORCED)

- **`main`** - Production branch, protected, only accepts PRs from `dev`
- **`dev`** - Development branch, ALL work happens here directly

### Rules

1. **NEVER create feature branches** - No `feature/*`, `bugfix/*`, or any other branches
2. **NEVER push directly to `main`** - Only PRs from `dev` are allowed
3. **ALL development happens on `dev`** - Commit directly to `dev` branch
4. **PR from `dev` to `main`** for releases only

### Workflow

```
dev ─── commit ─── commit ─── commit ─── PR ───► main (release)
```

### CI Enforcement

The CI workflow will FAIL if:

- A PR is opened from any branch other than `dev` to `main`
- Any branch other than `main` or `dev` is detected

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

1. Ensure you are on `dev` branch: `git checkout dev && git pull`
2. Write E2E tests first
3. Implement feature
4. Run `cargo fmt && cargo clippy -- -D warnings`
5. Run `cargo test`
6. Commit with descriptive message: `git commit -m "Add feature X"`
7. Push to dev: `git push origin dev`
8. When ready for release, open PR from `dev` to `main`

### Debugging Audio Issues

- Enable trace logging: `RUST_LOG=audiotester=trace cargo run`
- Check ASIO device list: Look for "ASIO" in device names
- Verify sample rate matches between devices

## Performance Targets

- Audio callback: < 1ms processing time
- UI updates: 60 FPS in stats window
- Memory: < 50MB typical usage
- CPU: < 5% during monitoring

## CI/CD Pipeline (CRITICAL)

**ALL compilation, testing, and releases MUST happen through GitHub CI/CD workflows.**

### What CI/CD Does

- `ci.yml` - Runs on every push to `dev` and PRs to `main`:
  - Format check (`cargo fmt`)
  - Lint check (`cargo clippy`)
  - Build (debug + release)
  - Unit tests
  - E2E tests
  - Branch policy enforcement

- `auto-release.yml` - Runs automatically on push to `main` (after PR merge):
  - Determines next version (auto-increments patch from latest tag)
  - Builds Windows release binary
  - Creates git tag and GitHub Release
  - Deploys to test machine (iem.lan) via self-hosted runner

- `deploy.yml` - Manual re-deploy only (`workflow_dispatch`):
  - Re-deploys a specific version to iem.lan

### Required GitHub Secrets

For automated deployment to work, these secrets must be configured:

| Secret         | Value     |
| -------------- | --------- |
| `IEM_HOST`     | `iem.lan` |
| `IEM_USER`     | `iem`     |
| `IEM_PASSWORD` | `iem`     |

### Workflow

```
Code → Push to dev → CI builds & tests → PR to main → CI validates → Merge → Auto version bump → Build → Release → Deploy to iem.lan
```

## Test Machine (iem.lan)

**This is a TESTING machine ONLY. NEVER set up development environment here.**

| Property | Value                                                 |
| -------- | ----------------------------------------------------- |
| Hostname | `iem.lan`                                             |
| Username | `iem`                                                 |
| Password | `iem`                                                 |
| Purpose  | **Testing compiled releases with real ASIO hardware** |

### What To Do On Test Machine

- Download releases from GitHub
- Run compiled `audiotester.exe`
- Test with real ASIO audio loopback
- Report issues back

### What NEVER To Do On Test Machine

- Install Rust or development tools
- Compile code
- Set up development environment
- Clone repositories for development

### Testing Workflow

1. CI/CD builds release on GitHub
2. Download release artifact from GitHub
3. Copy to test machine: `scp audiotester.exe iem@iem.lan:~/`
4. SSH and run: `ssh iem@iem.lan` then `.\audiotester.exe`
5. Test with real ASIO loopback
6. Report results
