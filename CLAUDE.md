# Audiotester Project Guidelines

## Project Overview

Windows ASIO audio testing application for monitoring professional audio paths (Dante, VBAN, VBMatrix). The application runs as a Tauri 2 desktop app with system tray, web UI via Leptos SSR on Axum, and remote access from any browser on the LAN.

## Quality Philosophy: Real Hardware E2E Testing

**CRITICAL: This project prioritizes E2E testing on REAL HARDWARE over all other testing methods.**

The primary quality gate is deployment to `iem.lan` with the VASIO-8 interface. Every push to `dev` MUST pass full hardware verification before a PR can be created. There are NO nightly builds, NO parameterized CI runs, NO separate test suites - ONE comprehensive flow validates everything.

### E2E Testing Hierarchy

| Priority        | Test Type      | Location                 | Purpose                      |
| --------------- | -------------- | ------------------------ | ---------------------------- |
| **1 (HIGHEST)** | Hardware E2E   | `iem.lan` CI smoke tests | Real ASIO + VASIO-8 loopback |
| **2**           | Playwright E2E | `e2e/*.spec.ts`          | Full browser/API flows       |
| **3**           | Rust E2E       | `tests/e2e_*.rs`         | Signal/latency/loss logic    |
| **4**           | Integration    | `tests/integration/`     | Component boundaries         |
| **5 (LOWEST)**  | Unit           | crate `tests` modules    | Isolated functions           |

**Always write E2E tests FIRST. Unit tests support E2E, not replace them.**

## Temporary Approvals

- **Agent is NOT approved to merge PRs** - User will manually verify dev deployments on iem.lan before approving PR merges to `main`. The dev-push CI pipeline deploys to iem.lan for testing, and the user will confirm the deployment is working before authorizing any merge.

## MANDATORY Self-Verification (STRICTLY ENFORCED)

**CRITICAL: The agent MUST verify ALL features/fixes on iem.lan BEFORE presenting them to the user.**

The user is NOT a tester. The user is here only to confirm that the agent's approach was correct. The agent is FULLY RESPONSIBLE for:

1. **Testing on real hardware** - Every feature MUST be verified working on iem.lan
2. **Checking logs** - Console output, tracing logs, error messages
3. **Verifying UI behavior** - Dashboard updates, tray icon changes, real-time data
4. **Testing edge cases** - Disconnect scenarios, error states, reconnection

### Self-Verification Workflow (MANDATORY)

```
1. Push to dev → CI deploys to iem.lan
2. SSH to iem.lan and verify:
   - Check process logs: Get-Process audiotester, check console output
   - Check dashboard: curl http://iem.lan:8920/api/v1/stats
   - Test feature manually via web UI or API
   - Verify real behavior matches expected behavior
3. If verification FAILS → Fix and repeat
4. If verification PASSES → ONLY THEN present to user
```

### What Agent MUST Verify Before Presenting to User

| Feature Type     | Verification Steps                                              |
| ---------------- | --------------------------------------------------------------- |
| API endpoint     | `curl` the endpoint, verify response format and values          |
| Dashboard UI     | Open http://iem.lan:8920, verify element displays correctly     |
| Signal detection | Disconnect loopback, verify status changes in dashboard AND API |
| Tray icon        | Check icon color changes with audio state                       |
| WebSocket        | Verify real-time updates in browser dev tools                   |

### NEVER Present Untested Features

The agent MUST NOT:

- Ask user to "test if it works"
- Deliver features without self-verification
- Assume CI passing means feature works correctly
- Skip manual verification because "it compiled"

**If the agent cannot verify a feature works, the agent MUST NOT claim it is ready.**

### Available Verification Tools on iem.lan

- SSH access: `ssh iem@iem.lan` (password: iem)
- Web UI: http://iem.lan:8920
- API: `curl http://iem.lan:8920/api/v1/stats`
- VBMatrix: Running on desktop, has API for routing control
- PowerShell: Full admin access for process inspection

## Agent Behavior

- Act as a highly skilled Rust/ASIO developer with expertise in real-time audio processing
- Prioritize E2E tests for every new feature - tests are the highest priority
- **NEVER create feature branches** - only `main` and `dev` branches exist
- **ALL work happens directly on `dev` branch** - commit and push to `dev`
- **ALL compilation, testing, and releases happen through GitHub CI/CD** - never compile locally on test machines
- **Test machines are for TESTING ONLY** - never install dev tools or set up dev environments on them
- Use idiomatic Rust patterns and maintain backward compatibility
- Keep solutions simple and focused - avoid over-engineering
- **ALWAYS provide a mergeable PR URL** - see "PR Delivery" section below
- **VERSION, RELEASE, AND DEPLOY ARE FULLY AUTOMATIC** - After PR merge to `main`:
  - `auto-release.yml` handles everything: version bump → build → release → deploy
  - Agent does NOT need to manually create tags or trigger workflows
  - Agent should monitor the auto-release workflow and report result to user

## PR Delivery (STRICTLY ENFORCED - NO EXCEPTIONS)

**CRITICAL: Every completed task MUST end with a GREEN, MERGEABLE PR URL delivered to the user.**

The user's ONLY deliverable is a PR they can click "Merge" on immediately. If the PR is not green, it is NOT delivered. If there is no PR URL, the work is NOT done.

### Required Workflow (MANDATORY - EVERY SINGLE TIME)

```
1. Push to dev
2. Wait for ALL CI jobs to pass (poll with gh run view until conclusion=success)
3. Verify CI is GREEN - if ANY job failed, fix and push again, repeat from step 1
4. Create PR: gh pr create --base main --head dev (or reuse existing open PR)
5. Verify PR is mergeable: gh pr view --json mergeable,reviewDecision,statusCheckRollup
6. Provide the PR URL as the LAST thing the user sees
```

### Rules

- **NEVER say "done" without a PR URL** - No PR URL = work not finished
- **NEVER present a RED/FAILING PR** - If CI is not green, FIX IT first. Do not tell the user "CI is running" and move on
- **NEVER ask the user to create the PR** - The agent creates the PR, always
- **NEVER skip waiting for CI** - The agent MUST poll CI status until ALL jobs complete
- **NEVER present a PR with merge conflicts** - Rebase dev on main if needed before creating PR
- **If a PR already exists** from dev to main, verify it is green and provide its URL
- **The PR URL must be the last line** of the final message, clearly visible, not buried
- **If CI fails after push** - the agent MUST fix the issue, push again, wait for green, THEN deliver

### What "Done" Looks Like

```
[Brief summary of changes]
[Verification results from iem.lan]

**PR: https://github.com/zbynekdrlik/audiotester/pull/N**
```

### What "Done" Does NOT Look Like (NEVER DO THIS)

```
"I've pushed the changes, CI is running..."          # NOT DONE - no PR
"Here's the PR: [url] - CI is still running"         # NOT DONE - not green
"Done! You can create a PR from dev to main"          # NOT DONE - agent must create PR
"The PR has some failing checks but..."               # NOT DONE - must be green
```

**The task is INCOMPLETE until a GREEN, MERGEABLE PR URL has been delivered to the user.**

## Code Standards

- Run `cargo fmt` before every commit
- Run `cargo clippy --workspace -- -D warnings` and fix all warnings
- All public functions must have doc comments with examples where appropriate
- E2E tests required for new features before merging
- Use `thiserror` for custom error types, `anyhow` for application errors
- Prefer `tracing` over `println!` for logging

## TDD - STRICTLY MANDATORY (NO EXCEPTIONS)

**CRITICAL: The agent MUST write E2E tests BEFORE writing ANY implementation code.**

This is not optional. Every bug fix, every feature, every change MUST have E2E tests written FIRST. Unit tests support E2E but do NOT replace them.

### TDD Workflow (MUST FOLLOW)

1. **RED - Write failing E2E tests FIRST**
   - Prefer Playwright tests (`e2e/*.spec.ts`) for user-facing features
   - Use Rust E2E tests (`tests/e2e_*.rs`) for signal/audio logic
   - Tests MUST fail (compilation errors count as failures)
   - Tests MUST exercise the FULL user flow, not just isolated functions

2. **GREEN - Implement minimal code**
   - Write ONLY enough code to make E2E tests pass
   - Do NOT write "extra" code not covered by tests
   - Run tests after EVERY change

3. **REFACTOR - Clean up**
   - Keep tests green while improving code
   - Run full test suite before committing

### Test Priority by Issue Type (ALWAYS E2E FIRST)

| Issue Type   | Primary Test                 | Secondary Test              | Example                       |
| ------------ | ---------------------------- | --------------------------- | ----------------------------- |
| UI Bug       | `e2e/*.spec.ts`              | `tests/e2e_*.rs`            | Dashboard not updating        |
| ASIO Error   | `e2e/hardware-smoke.spec.ts` | `tests/e2e_reconnection.rs` | Reconnection on buffer change |
| API Bug      | `e2e/api.spec.ts`            | `tests/e2e_dashboard.rs`    | Device info missing           |
| WebSocket    | `e2e/websocket.spec.ts`      | -                           | Stats not streaming           |
| Tray Icon    | `tests/e2e_tray.rs`          | Unit tests                  | Color not changing            |
| Signal Logic | `tests/e2e_signal.rs`        | Unit tests                  | Latency calculation           |

**The PRIMARY test is ALWAYS E2E. Unit tests are SECONDARY support.**

### What E2E Tests MUST Verify

For EVERY feature/bugfix, your E2E test must cover:

1. **Full user flow** - From user action to visible result
2. **API contract** - Correct request/response format
3. **WebSocket propagation** - Real-time updates work
4. **State after refresh** - Data survives page reload
5. **Error states** - Graceful handling of failures
6. **Hardware interaction** - Works on iem.lan (if applicable)

### TDD Commit Pattern

```
git commit -m "test: add failing E2E tests for feature X"  # RED - tests fail
git commit -m "feat: implement feature X"                   # GREEN - tests pass
git commit -m "refactor: clean up feature X"                # REFACTOR
```

### ENFORCEMENT

The agent MUST NOT:

- Write implementation code before E2E tests
- Write ONLY unit tests (E2E is mandatory)
- Skip tests because "it's a small change"
- Write tests after implementation (defeats the purpose)
- Ignore failing tests

**If CI fails on iem.lan hardware, the PR CANNOT be created.** Fix the issue first.

## Strict CI/CD Requirements

### Zero Tolerance Policy

- **NO skipped tests** - All tests must run, no `#[ignore]` without explicit approval
- **NO false positives** - Tests must fail on actual regressions, never pass incorrectly
- **NO ignored failures** - Every CI job must pass for PR to be green
- **Hardware verification required** - Deploy smoke tests on iem.lan must pass
- **NO PR before CI passes** - Agent must wait for full CI completion before creating PR

### What "CI Passes" Means

For a PR to be created, ALL of the following must be true:

1. `git push origin dev` completes successfully
2. ALL GitHub Actions jobs show green checkmarks:
   - `branch-check` ✅
   - `main-sync` ✅
   - `fmt` ✅
   - `clippy` ✅
   - `build` ✅
   - `test` ✅
   - `e2e` ✅
   - `playwright` ✅
   - `build-dev` ✅
   - `deploy-dev` ✅
   - `ci-success` ✅
3. Hardware smoke tests on iem.lan pass:
   - VASIO-8 device connected
   - Monitoring active
   - Latency in range (0-100ms)
   - Sample loss acceptable

**If ANY job fails, fix and push again. Do NOT create PR until CI is fully green.**

### CI Gate Rules (ALL BLOCKING)

| Check      | Blocking? | What It Validates                         |
| ---------- | --------- | ----------------------------------------- |
| fmt        | **YES**   | Code formatting (`cargo fmt --check`)     |
| clippy     | **YES**   | No warnings (`-D warnings`)               |
| build      | **YES**   | Debug + release compilation               |
| test       | **YES**   | All workspace unit + integration tests    |
| e2e        | **YES**   | Rust E2E tests (`tests/e2e_*.rs`)         |
| playwright | **YES**   | Browser E2E tests (`e2e/*.spec.ts`)       |
| build-dev  | **YES**   | Release binary builds successfully        |
| deploy-dev | **YES**   | Deploys to iem.lan + hardware smoke tests |
| ci-success | **YES**   | All jobs passed - final gate              |

### Hardware Smoke Test Criteria

The `deploy-dev` job validates on REAL hardware:

| Check           | Threshold        | What It Catches    |
| --------------- | ---------------- | ------------------ |
| Process running | Must be running  | Crash on startup   |
| Web server      | Response < 20s   | Server deadlock    |
| API status      | Valid JSON       | Response format    |
| Device          | Contains "VASIO" | Device selection   |
| Monitoring      | `true`           | Auto-start failure |
| Latency         | 0-100ms          | Aliasing, cycling  |
| Sample loss     | < 1000/10s       | Buffer issues      |
| Two-point       | Stable           | Drift detection    |

### Audio Measurement Quality Standards

- **Latency**: Must be stable 1-50ms range for loopback, no cycling to 341ms/682ms
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

### E2E-First Development (MANDATORY)

**Every feature/bugfix MUST include E2E tests that exercise the FULL user-facing flow.**

The goal is comprehensive end-to-end coverage that prevents regressions on real hardware. Writing isolated unit tests without corresponding E2E tests is NOT acceptable.

### Playwright E2E (Browser + API) - `e2e/*.spec.ts`

| Test File                 | Coverage                                              |
| ------------------------- | ----------------------------------------------------- |
| `dashboard.spec.ts`       | Full dashboard load, WebSocket stats, graph rendering |
| `api.spec.ts`             | REST API endpoints, status, stats, devices, config    |
| `websocket.spec.ts`       | Real-time updates, reconnection, message format       |
| `settings.spec.ts`        | Device selection, sample rate, buffer size            |
| `monitoring-flow.spec.ts` | Start/stop monitoring, state transitions              |
| `hardware-smoke.spec.ts`  | VASIO-8 specific checks (CI runs on iem.lan)          |

**Playwright tests MUST simulate real user workflows, not just API calls.**

### Rust E2E Tests - `tests/e2e_*.rs`

| Test File                 | Coverage                                |
| ------------------------- | --------------------------------------- |
| `e2e_signal.rs`           | MLS generation, properties, correlation |
| `e2e_latency.rs`          | Latency measurement accuracy, bounds    |
| `e2e_loss.rs`             | Sample loss detection, counting         |
| `e2e_tray.rs`             | Tray icon status logic                  |
| `e2e_tray_integration.rs` | Full tray status flow                   |
| `e2e_dashboard.rs`        | Dashboard API response structure        |
| `e2e_reconnection.rs`     | ASIO reconnection, state preservation   |

### Hardware Smoke Tests (CI on iem.lan)

These tests run in CI on every `dev` push and validate against REAL hardware:

```yaml
# From ci.yml deploy-dev job
- Verify VASIO-8 device is connected and selected
- Verify monitoring is active
- Verify latency is in expected range (0-100ms)
- Verify sample loss rate is acceptable (<1000/10s)
- Two-point measurement to detect cycling/aliasing
```

**Hardware smoke tests are BLOCKING - PR cannot be created until they pass.**

### What E2E Tests MUST Cover

When adding a new feature, your E2E test MUST verify:

1. **User-visible behavior** - What the user sees/clicks
2. **API contract** - Request/response format
3. **WebSocket flow** - Real-time update propagation
4. **State persistence** - Data survives page refresh
5. **Error handling** - Graceful failure modes
6. **Hardware integration** - Works on iem.lan (if applicable)

### Running Tests

```bash
# FULL CI-equivalent flow (always run before PR)
cargo fmt --all --check && \
cargo clippy --workspace --all-targets --all-features -- -D warnings && \
cargo test --workspace && \
npx playwright test

# Specific test suites
cargo test --test 'e2e_*' -- --nocapture  # Rust E2E
npx playwright test e2e/dashboard.spec.ts  # Single Playwright
npx playwright test --headed                # Visual debugging
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

### Adding a New Feature (STRICT WORKFLOW)

**Every feature follows this EXACT flow. No shortcuts.**

```
1. Sync dev branch
   └─> git checkout dev && git pull origin dev

2. Write failing E2E tests (TDD RED)
   ├─> Playwright: e2e/[feature].spec.ts
   └─> Rust: tests/e2e_[feature].rs

3. Verify tests fail
   └─> npx playwright test && cargo test --test 'e2e_*'

4. Implement feature (TDD GREEN)
   └─> Write minimal code to make tests pass

5. Verify tests pass locally
   └─> cargo fmt && cargo clippy --workspace -- -D warnings && cargo test --workspace && npx playwright test

6. Commit (E2E test commit FIRST)
   ├─> git commit -m "test: add E2E tests for feature X"
   └─> git commit -m "feat: implement feature X"

7. Push to dev (triggers FULL CI)
   └─> git push origin dev

8. WAIT for CI to complete
   ├─> All tests must pass
   ├─> deploy-dev must succeed
   └─> Hardware smoke tests must pass on iem.lan

9. Verify on iem.lan manually
   └─> Open http://iem.lan:8920 and test feature

10. Create PR from dev to main
    └─> gh pr create --base main --head dev

11. User verifies and merges
    └─> Auto-release deploys to production
```

**Steps 7-8 are BLOCKING. If CI fails, fix and push again. NO PR until CI passes.**

### Fixing a Bug (STRICT WORKFLOW)

```
1. Sync dev and understand the bug
2. Write E2E test that reproduces the bug (must FAIL)
3. Verify test fails with expected error
4. Fix the bug
5. Verify test passes
6. Run full test suite
7. Commit: "test: reproduce bug X" then "fix: resolve bug X"
8. Push to dev
9. WAIT for CI including iem.lan hardware tests
10. Create PR only after CI passes
```

### Debugging Audio Issues

- Enable trace logging: `RUST_LOG=audiotester=trace cargo run -p audiotester-app`
- Check ASIO device list: Look for "ASIO" in device names
- Verify sample rate matches between devices
- Check web UI at http://localhost:8920 (local) or http://iem.lan:8920 (test machine)
- Monitor CI deploy-dev job for hardware test results

## Performance Targets

- Audio callback: < 1ms processing time
- Web UI: responsive on mobile browsers
- Memory: < 50MB typical usage
- CPU: < 5% during monitoring

## CI/CD Pipeline (CRITICAL)

**ALL compilation, testing, and releases MUST happen through GitHub CI/CD workflows.**

### Single Comprehensive Flow (NO Nightly/Parameterized CI)

This project uses ONE workflow that does EVERYTHING. There are no separate:

- Nightly builds (nobody runs them)
- Parameterized matrix jobs (complexity without benefit)
- Manual test triggers (gets forgotten)
- Separate security scans (integrated in main flow)

**Every `dev` push runs the FULL validation including hardware deployment.**

### CI Jobs (ALL BLOCKING)

| Job            | Runs On    | What It Does                               |
| -------------- | ---------- | ------------------------------------------ |
| `branch-check` | `push/PR`  | Enforce main/dev only policy               |
| `main-sync`    | `dev push` | Ensure dev includes all main commits       |
| `fmt`          | `all`      | Format check (`cargo fmt --all --check`)   |
| `clippy`       | `all`      | Lint check (`cargo clippy -- -D warnings`) |
| `build`        | `all`      | Debug + release compilation                |
| `test`         | `all`      | All workspace unit + integration tests     |
| `e2e`          | `all`      | Rust E2E tests (`tests/e2e_*.rs`)          |
| `playwright`   | `all`      | Browser E2E tests (`e2e/*.spec.ts`)        |
| `build-dev`    | `dev push` | Build release binary                       |
| `deploy-dev`   | `dev push` | Deploy to iem.lan + hardware smoke tests   |
| `ci-success`   | `all`      | Final gate - ALL jobs must pass            |

**STRICT: Every job is blocking. There are no "allowed to fail" jobs.**

### Hardware Verification Flow

```
dev push → build → deploy to iem.lan → smoke tests on VASIO-8 → PASS required
```

The `deploy-dev` job performs these hardware validations:

1. Process starts and stays running
2. Web server responds within 20s
3. API returns valid status
4. VASIO-8 device detected and selected
5. Monitoring active
6. Latency in range (0-100ms, catches aliasing)
7. Sample loss acceptable (<1000 in 10s)
8. Two-point measurement (detects cycling)

### Auto-Release Flow (main only)

```
PR merge to main → version++ → build → release → deploy → smoke tests
```

Fully automatic - NO manual tagging or version bumping required.

### State-of-the-Art CI Improvements (Implemented)

| Feature                | Status | Description                       |
| ---------------------- | ------ | --------------------------------- |
| Single flow            | ✅     | One workflow validates everything |
| Hardware gate          | ✅     | Real ASIO tested every commit     |
| Branch protection      | ✅     | Only dev→main PRs allowed         |
| Main sync check        | ✅     | Dev must include all main commits |
| Auto-versioning        | ✅     | Semver auto-increment             |
| Self-hosted runner     | ✅     | Deploys to iem.lan                |
| Scheduled task restart | ✅     | Auto-start on machine reboot      |

### Future CI Improvements to Consider

| Feature                | Priority | Description                       |
| ---------------------- | -------- | --------------------------------- |
| Cargo audit            | HIGH     | Check for vulnerable dependencies |
| SBOM generation        | MEDIUM   | Software bill of materials        |
| Binary size tracking   | MEDIUM   | Alert on size regression          |
| Performance benchmarks | LOW      | Catch latency regressions         |
| Coverage gates         | LOW      | Minimum coverage threshold        |

### Required GitHub Secrets

| Secret         | Value     |
| -------------- | --------- |
| `IEM_HOST`     | `iem.lan` |
| `IEM_USER`     | `iem`     |
| `IEM_PASSWORD` | `iem`     |

### The Golden Rule

**If CI passes on `dev`, the code is ready for production.**

There is no "works on my machine" - if iem.lan hardware tests pass, the code works.

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
