# Audiotester Project Guidelines

## Project Overview

Windows ASIO audio testing application for monitoring professional audio paths (Dante, VBAN, VBMatrix). The application runs as a Tauri 2 desktop app with system tray, web UI via Leptos SSR on Axum, and remote access from any browser on the LAN.

## Temporary Approvals

- **Agent is NOT approved to merge PRs** - User will manually verify dev deployments on iem.lan before approving PR merges to `main`. The dev-push CI pipeline deploys to iem.lan for testing, and the user will confirm the deployment is working before authorizing any merge.

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
- Run `cargo clippy --workspace -- -D warnings` and fix all warnings
- All public functions must have doc comments with examples where appropriate
- E2E tests required for new features before merging
- Use `thiserror` for custom error types, `anyhow` for application errors
- Prefer `tracing` over `println!` for logging

## TDD Mandatory Process

Every feature MUST follow this strict TDD workflow:

1. **Write failing tests FIRST** - Create the test file before any implementation
2. **Run tests to verify they fail** - Tests must fail for the right reasons (compilation errors for missing types/functions are acceptable failures)
3. **Implement minimal code** - Write only enough code to make tests pass
4. **Refactor** - Clean up while keeping tests green
5. **Never skip tests** - No `#[ignore]` without explicit approval

### Test File Naming Convention

| Test Type   | File Pattern             | Example                           |
| ----------- | ------------------------ | --------------------------------- |
| E2E         | `tests/e2e_*.rs`         | `tests/e2e_tray.rs`               |
| Integration | `tests/integration/*.rs` | `tests/integration/audio_loop.rs` |
| Unit        | Inside `mod tests`       | `#[cfg(test)] mod tests { ... }`  |

### TDD Commit Pattern

```
git commit -m "test: add e2e tests for feature X"   # RED phase
git commit -m "feat: implement feature X"            # GREEN phase
git commit -m "refactor: clean up feature X"         # REFACTOR phase
```

## Strict CI/CD Requirements

### Zero Tolerance Policy

- **NO skipped tests** - All tests must run, no `#[ignore]` without explicit approval
- **NO false positives** - Tests must fail on actual regressions, never pass incorrectly
- **NO ignored failures** - Every CI job must pass for PR to be green
- **Hardware verification required** - Deploy smoke tests on iem.lan must pass

### Meta-Test Requirements

- Run `tests/meta_tests.rs` to verify test suite integrity
- No PRs merge if any tests are skipped or ignored
- Latency measurements must be bounded (no aliasing artifacts)
- Loss detection must be accurate (no false positives from latency cycling)

### CI Gate Rules

| Check      | Blocking? | Description                   |
| ---------- | --------- | ----------------------------- |
| fmt        | Yes       | Code must be formatted        |
| clippy     | Yes       | No warnings allowed           |
| build      | Yes       | Must compile                  |
| test       | Yes       | All unit tests pass           |
| e2e        | Yes       | All E2E tests pass            |
| playwright | Yes       | All browser tests pass        |
| deploy-dev | Yes       | Hardware smoke tests pass     |
| meta-tests | Yes       | Test suite integrity verified |

### Audio Measurement Quality Standards

- **Latency**: Must be stable 1-50ms range for loopback, no cycling
- **Sample loss**: Must be zero for healthy connections
- **Confidence**: Cross-correlation confidence > 0.5 for valid measurements
- **Tray icon**: Must reflect actual status (green/orange/red/gray)

## Architecture

```
audiotester/                       # Workspace root
├── Cargo.toml                     # Workspace definition
├── src/lib.rs                     # Re-exports audiotester-core (for test compat)
├── tests/                         # Existing E2E + integration tests
│   ├── e2e_signal.rs
│   ├── e2e_latency.rs
│   ├── e2e_loss.rs
│   └── integration/audio_loop.rs
│
├── crates/
│   ├── audiotester-core/          # Core audio engine + signal processing
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── audio/
│   │       │   ├── engine.rs      # ASIO device management, streams
│   │       │   ├── signal.rs      # MLS generation
│   │       │   └── analyzer.rs    # FFT cross-correlation
│   │       └── stats/
│   │           └── store.rs       # Time-series data storage
│   │
│   └── audiotester-server/        # Axum + Leptos SSR web server
│       └── src/
│           ├── lib.rs             # Router, AppState, EngineHandle
│           ├── api.rs             # REST endpoints (/api/v1/*)
│           ├── ws.rs              # WebSocket handler
│           └── ui/
│               ├── dashboard.rs   # Leptos SSR dashboard page
│               ├── settings.rs    # Leptos SSR settings page
│               ├── components/    # Reusable Leptos components
│               ├── styles/        # CSS (included via include_str!)
│               └── scripts/       # JS (included via include_str!)
│
└── src-tauri/                     # Tauri 2 desktop shell
    ├── tauri.conf.json            # NSIS installer, window config
    └── src/
        ├── main.rs                # Entry point
        ├── lib.rs                 # App setup, server spawn, monitoring loop
        └── tray.rs                # Tray icon + menu
```

### Access Paths

```
Axum Server (0.0.0.0:8920)
├── Leptos SSR pages (server-rendered HTML + embedded JS)
├── REST API (/api/v1/stats, /api/v1/devices, ...)
├── WebSocket (/api/v1/ws) for real-time push
│
├── Tauri Webview → localhost:8920        ← desktop (tray click)
├── Chrome on laptop → iem.lan:8920      ← remote debugging
└── Phone browser → 192.168.x.x:8920    ← remote monitoring
```

## Key Dependencies

| Crate                 | Purpose                         |
| --------------------- | ------------------------------- |
| `cpal` (ASIO feature) | Audio I/O with ASIO backend     |
| `tauri` 2             | Desktop shell, tray, installer  |
| `axum` 0.8            | Web server, REST API, WebSocket |
| `leptos` 0.7 (SSR)    | Server-side HTML rendering      |
| `ringbuf`             | Lock-free audio buffers         |
| `rustfft`             | FFT-based cross-correlation     |
| `tower-http`          | Static file serving             |

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

- `tests/e2e_signal.rs` - MLS generation and properties (10 tests)
- `tests/e2e_latency.rs` - Latency measurement accuracy (11 tests)
- `tests/e2e_loss.rs` - Sample loss detection (11 tests)

### Integration Tests

- `tests/integration/audio_loop.rs` - Full audio path (mocked ASIO)

### Unit Tests

- `crates/audiotester-core/` - Audio engine, signal, analyzer, stats (21 tests)
- `crates/audiotester-server/` - API serialization tests (4 tests)

### Running Tests

```bash
# All workspace tests
cargo test --workspace

# Core library only
cargo test -p audiotester-core

# Server only
cargo test -p audiotester-server

# E2E only
cargo test --test 'e2e_*'

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
4. Run `cargo fmt && cargo clippy --workspace -- -D warnings`
5. Run `cargo test --workspace`
6. Commit with descriptive message: `git commit -m "Add feature X"`
7. Push to dev: `git push origin dev`
8. When ready for release, open PR from `dev` to `main`

### Debugging Audio Issues

- Enable trace logging: `RUST_LOG=audiotester=trace cargo run -p audiotester-app`
- Check ASIO device list: Look for "ASIO" in device names
- Verify sample rate matches between devices
- Check web UI at http://localhost:8920

## Performance Targets

- Audio callback: < 1ms processing time
- Web UI: responsive on mobile browsers
- Memory: < 50MB typical usage
- CPU: < 5% during monitoring

## CI/CD Pipeline (CRITICAL)

**ALL compilation, testing, and releases MUST happen through GitHub CI/CD workflows.**

### What CI/CD Does

- `ci.yml` - Runs on every push to `dev` and PRs to `main`:
  - Format check (`cargo fmt --all`)
  - Lint check (`cargo clippy --workspace`)
  - Build workspace (debug + release)
  - Unit tests (all workspace crates)
  - E2E tests
  - Branch policy enforcement

- `auto-release.yml` - Runs automatically on push to `main` (after PR merge):
  - Determines next version (auto-increments patch from latest tag)
  - Builds Tauri app release binary
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
| Web UI   | `http://iem.lan:8920`                                 |

### What To Do On Test Machine

- Download releases from GitHub
- Run compiled `audiotester.exe`
- Test with real ASIO audio loopback
- Access web UI at http://iem.lan:8920
- Report issues back

### What NEVER To Do On Test Machine

- Install Rust or development tools
- Compile code
- Set up development environment
- Clone repositories for development

### Testing Workflow

1. CI/CD builds release on GitHub
2. Auto-deploy copies to iem.lan
3. Access web UI: http://iem.lan:8920
4. Test with real ASIO loopback
5. Report results
