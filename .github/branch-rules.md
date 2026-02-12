# Branch Protection Rules

This document describes the branch protection configuration for the audiotester repository.

## Main Branch (`main`)

The `main` branch is protected with the following rules:

### Required Status Checks

Before merging to `main`, all of these checks must pass:

- `fmt` - Code formatting check (`cargo fmt --check`)
- `clippy` - Linting check (`cargo clippy -- -D warnings`)
- `build` - Debug and release builds
- `test` - Unit and integration tests
- `e2e` - End-to-end tests

### Merge Requirements

- **Require pull request before merging**: Direct pushes to `main` are not allowed
- **Require linear history**: Force merges and merge commits that don't maintain linear history are rejected
- **Require branches to be up to date**: The PR branch must be up to date with `main` before merging
- **Restrict merges to `dev` branch only**: Only PRs from the `dev` branch can be merged to `main`

### Additional Protections

- **Do not allow force pushes**: History cannot be rewritten
- **Do not allow deletions**: The `main` branch cannot be deleted

## Dev Branch (`dev`)

The `dev` branch has lighter protection:

### Required Status Checks

- `fmt` - Code formatting check
- `clippy` - Linting check
- `build` - Build check
- `test` - Unit and integration tests
- `e2e` - End-to-end tests

### Merge Requirements

- **Require pull request before merging**: Direct pushes are not allowed
- Feature branches should be merged to `dev` via PR

## Workflow

```
feature/xxx ─── PR ───► dev ─── PR ───► main
                         │
bugfix/xxx ────── PR ────┘
```

1. Create feature branches from `dev`
2. Open PR to merge feature branch into `dev`
3. All CI checks must pass
4. After review, merge to `dev`
5. Periodically, open PR from `dev` to `main` for release
6. All CI checks must pass on the `dev` → `main` PR
7. Merge to `main` triggers release workflow (if tagged)

## Setting Up Branch Protection

To configure these rules via GitHub CLI:

```bash
# Main branch protection
gh api repos/{owner}/{repo}/branches/main/protection -X PUT \
  -H "Accept: application/vnd.github+json" \
  -f required_status_checks='{"strict":true,"contexts":["fmt","clippy","build","test","e2e"]}' \
  -f enforce_admins=true \
  -f required_pull_request_reviews='{"required_approving_review_count":0}' \
  -f restrictions=null \
  -f allow_force_pushes=false \
  -f allow_deletions=false \
  -f required_linear_history=true

# Dev branch protection
gh api repos/{owner}/{repo}/branches/dev/protection -X PUT \
  -H "Accept: application/vnd.github+json" \
  -f required_status_checks='{"strict":true,"contexts":["fmt","clippy","build","test","e2e"]}' \
  -f enforce_admins=false \
  -f required_pull_request_reviews=null \
  -f restrictions=null
```
