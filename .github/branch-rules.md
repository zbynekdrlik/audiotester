# Branch Policy

## CRITICAL: Two Branches Only

This repository uses a **strict two-branch policy**. Only these branches are allowed:

| Branch | Purpose                        |
| ------ | ------------------------------ |
| `main` | Production releases, protected |
| `dev`  | All development work           |

**NO OTHER BRANCHES ARE PERMITTED** - no `feature/*`, `bugfix/*`, `hotfix/*`, or any other branches.

## Workflow

```
dev ─── commit ─── commit ─── commit ─── PR ───► main
                                          │
                                    (release tag)
```

1. All development happens directly on `dev`
2. Commit and push to `dev` branch
3. When ready for release, open PR from `dev` to `main`
4. CI checks must pass
5. Merge to `main` and tag for release

## Main Branch (`main`)

### Protection Rules

- **Require pull request before merging**: Direct pushes are blocked
- **Only accept PRs from `dev`**: PRs from any other branch will fail CI
- **Require status checks**: fmt, clippy, build, test, e2e, branch-check
- **Require linear history**: Clean commit history
- **No force pushes**: History cannot be rewritten
- **No deletions**: Branch cannot be deleted

## Dev Branch (`dev`)

### Rules

- All commits go here directly
- Push access for contributors
- CI runs on every push
- This is the ONLY branch you work on

## CI Enforcement

The `branch-check` job in CI enforces this policy:

```yaml
# Fails if PR to main is not from dev
- name: Verify PR source branch
  if: github.event_name == 'pull_request' && github.base_ref == 'main'
  run: |
    if [[ "${{ github.head_ref }}" != "dev" ]]; then
      echo "ERROR: PRs to main must come from dev branch only!"
      exit 1
    fi
```

## Why This Policy?

1. **Simplicity**: No branch management overhead
2. **Single source of truth**: `dev` always has latest code
3. **Clean releases**: `main` only gets tested, reviewed code
4. **No stale branches**: Only 2 branches to maintain
5. **Clear workflow**: No confusion about where to work

## For AI Agents

**IMPORTANT**: When working on this codebase:

1. NEVER suggest creating feature branches
2. ALWAYS work directly on `dev`
3. NEVER push to `main` directly
4. Use `git checkout dev && git pull` before starting work
5. Commit and push to `dev` when done
