# CI/CD guide

How shakespAIre is tested, versioned, built, and released. Everything here is automatic except two human decisions: **merging a Release pull request** (when to ship) and **approving the publish** (the last check before it goes public).

## The whole pipeline at a glance

```
 you open a pull request
        │
        ▼
   CI runs (ci.yml) ──► required check "CI" must be green ──► you merge (squash)
                                                                   │
                                                                   ▼
                                     Release workflow (release-please.yml) runs on main
                                                                   │
                                     ┌─────────────────────────────┴───────────────┐
                                     ▼                                             ▼
                        nothing to release                            opens/updates the "Release" PR
                        (only ci:/docs:/chore: commits)               (next version + CHANGELOG.md)
                                                                                   │
                                                                     you merge the Release PR
                                                                                   │
                                                                                   ▼
                                                          tag vX.Y.Z + draft GitHub release are created
                                                                                   │
                                                                                   ▼
                                                     installers built for macOS, Windows, Linux (release.yml)
                                                                                   │
                                                                                   ▼
                                               you approve "Publish release" (environment: release)
                                                                                   │
                                                                                   ▼
                                     release goes public, latest.json goes live, installed apps can self-update
```

## Files

| File | Purpose |
|---|---|
| [`workflows/ci.yml`](workflows/ci.yml) | Checks every pull request and every push to `main`. |
| [`workflows/release-please.yml`](workflows/release-please.yml) | The **Release** workflow: prepares the Release PR, then builds and publishes. |
| [`workflows/release.yml`](workflows/release.yml) | **Build installers**: builds and uploads the installers for one tag. Called by the workflow above, or run by hand. |
| [`workflows/dependabot-auto-merge.yml`](workflows/dependabot-auto-merge.yml) | Merges safe dependency updates once CI is green. |
| [`dependabot.yml`](dependabot.yml) | Dependency update schedule and rules. |
| [`rulesets/main-protection.json`](rulesets/main-protection.json) | Branch protection for `main`, importable in the GitHub UI. |
| [`../release-please-config.json`](../release-please-config.json) | How versions and the changelog are produced. |
| [`../.release-please-manifest.json`](../.release-please-manifest.json) | The last released version (managed by the bot). |
| [`../docs/RELEASING.md`](../docs/RELEASING.md) | Signing, notarization, and the updater key. |

## CI (`ci.yml`)

Runs on pull requests (opened, edited, updated, reopened) and on pushes to `main`. A newer push to the same branch cancels the older run.

| Job | What it does | Runs on |
|---|---|---|
| **Frontend** | `npm ci`, `npm run check` (type-check), `npm run build`. | Linux |
| **Rust lint** | `cargo fmt --check` and `cargo clippy --all-targets -- -D warnings` (warnings are errors). | Linux |
| **Rust tests** | `cargo test`. | Linux |
| **Compile check** | `cargo check` on macOS and on Windows. About half the Rust code is platform-specific and cannot be compiled on Linux. | macOS, Windows |
| **Conventional PR title** | The pull request title must be `type: description`. Only on pull requests. | Linux |
| **CI** | One summary job. It passes only if every job above passed (skipped counts as passed). **This is the single required check** on `main`. | Linux |

Why it is fast:

- The jobs run **in parallel**, so the wait is the slowest job, not the sum.
- Tauri only needs its `frontendDist` folder to *exist* when Rust compiles, so the Rust jobs create an empty `dist/` and don't wait for the frontend build.
- Debug info is switched off for CI builds (`CARGO_PROFILE_DEV_DEBUG=0`), which compiles faster and keeps caches small.
- The Rust cache is **saved only on `main`**; pull requests read it. That keeps it warm and inside GitHub's cache size limit. The macOS/Windows compile job also runs after every merge to keep those caches fresh.

Allowed title types: `feat`, `fix`, `perf`, `refactor`, `docs`, `build`, `ci`, `test`, `style`, `chore`, `revert`.

## Releases

### Commit messages decide the version

The squash-merged pull request title becomes the commit message, and the release bot reads it:

| Type | Version change (while below 1.0) | In the changelog |
|---|---|---|
| `fix:` `perf:` | patch (0.2.1 → 0.2.2) | yes |
| `feat:` | minor (0.2.2 → 0.3.0) | yes |
| `feat!:` or a `BREAKING CHANGE:` footer | minor while below 1.0, major after | yes |
| `docs:` `ci:` `chore:` `refactor:` `build:` `test:` `style:` | no release | hidden |

With several commits, the biggest change wins. To force a specific version, put `Release-As: 1.0.0` on its own line at the end of a commit message. That commit must reach `main` with the line intact, so merge that pull request with **Rebase and merge**, not squash.

### The Release workflow (`release-please.yml`)

Runs on every push to `main`. Three jobs:

1. **Prepare release** — opens or updates the **Release pull request**, or, when that pull request was just merged, creates the tag `vX.Y.Z` and a **draft** GitHub release with the changelog as its notes. The Release PR bumps the version in all five places at once: `package.json`, `package-lock.json`, `src-tauri/Cargo.toml`, `src-tauri/Cargo.lock`, `src-tauri/tauri.conf.json`.
2. **Build installers** — calls `release.yml` for the new tag, only when a release was created. It passes the repository secrets on with `secrets: inherit`.
3. **Publish release** — runs in the **`release`** environment. If you added yourself as a required reviewer there, it waits for your approval, then makes the draft public and marks it *latest*. Publishing is what makes `latest.json` reachable, so installed apps only ever see published releases.

A tag created by a workflow does not start other workflows, which is why the build is chained inside this workflow instead of being triggered by the tag.

### Building installers (`release.yml`)

Called by the Release workflow, or started by hand (**Actions → Build installers → Run workflow**, then type the tag, for example `v0.2.2`).

1. **Make sure the draft release exists** — creates it if missing, so the three builds don't race to create it.
2. **Build** (three jobs at once): macOS Apple Silicon, Windows x64, Linux x64 (Ubuntu 22.04, to keep the glibc requirement low). Each one installs dependencies, builds with `tauri-action`, signs the update files with the updater key, and uploads installers, `.sig` files, and `latest.json` to the draft.

There are deliberately no type-check or test steps here: CI already gates everything merged to `main`, and releases are built from `main`.

### Manual operations

| I want to… | Do this |
|---|---|
| Rebuild the installers for a tag | **Actions → Build installers → Run workflow**, enter the tag. |
| Retry after a failed build | Open the failed run and click **Re-run failed jobs**. |
| Force a version | Add `Release-As: X.Y.Z` to a commit message (merge with *Rebase and merge*). |
| Ship a release now | Merge the open Release pull request. |

## Dependency updates

- [`dependabot.yml`](dependabot.yml) checks npm, Rust, and GitHub Actions **once a month**, grouped into one pull request per ecosystem. Titles use `build(deps):` or `ci:` so they pass the title check.
- **Major versions of npm and Rust dependencies are never proposed** — they are breaking changes and are upgraded by hand.
- **Tauri is patch-only.** The Rust `tauri` crate and the `@tauri-apps/*` npm packages must share the same minor version (a mismatch stops `tauri dev`), and Dependabot cannot update both ecosystems together. Bump Tauri minors manually, both sides in one pull request.
- [`dependabot-auto-merge.yml`](workflows/dependabot-auto-merge.yml) turns on auto-merge (squash) for Dependabot's non-major updates. They merge on their own once CI is green.

## Branch protection

[`rulesets/main-protection.json`](rulesets/main-protection.json) protects `main`: changes must go through a pull request, the **CI** check must pass, history stays linear, and `main` cannot be deleted or force-pushed. Repository admins can bypass it for emergencies.

## One-time repository setup

| Where | What |
|---|---|
| **Settings → Rules → Rulesets** | *New ruleset → Import a ruleset* and choose `rulesets/main-protection.json`. Import it after CI has run once so the **CI** check exists. |
| **Settings → General** | Turn on **Allow auto-merge** and **Automatically delete head branches**. |
| **Settings → Actions → General** | Under *Workflow permissions*, turn on **Allow GitHub Actions to create and approve pull requests**. |
| **Settings → Environments** | Create an environment named **`release`** and add yourself under *Required reviewers*. This is the final approval gate. |
| **Settings → Secrets and variables → Actions** | Add the secrets below. |

### Secrets

| Secret | Required | Purpose |
|---|---|---|
| `TAURI_SIGNING_PRIVATE_KEY` | **Yes** | Signs the update files. The full contents of the private key file, on one line. The public half is in `src-tauri/tauri.conf.json`. Without it the build fails. Back the key up: if it is lost, installed apps can never accept another update. |
| `RELEASE_PLEASE_TOKEN` | Recommended | A fine-grained personal access token limited to this repository (*Contents* and *Pull requests* read/write). Pull requests opened with the default token do not start CI, so without it the Release pull request has no **CI** check and you must merge it as an admin using the ruleset bypass. |
| `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` | **No — leave unset** | The key has no password. A stray value here makes signing fail with a misleading error. |

`GITHUB_TOKEN` is provided automatically; no setup needed.

## Troubleshooting

| Symptom | Cause and fix |
|---|---|
| `failed to decode secret key: incorrect updater private key password: Missing comment in secret key` on every platform | The build did not receive the signing key. A reusable workflow only sees secrets that the caller passes on; `release-please.yml` must have `secrets: inherit` on its `build` job. If it does, re-save `TAURI_SIGNING_PRIVATE_KEY` (one line, 348 characters) and delete any `…_PASSWORD` secret. |
| `"releaseName" not set but required to create release` / `Release not found or created` | The draft release did not exist and the three builds raced to create it. The `prepare` job in `release.yml` prevents this. Re-run **Build installers** with the tag. |
| A release has no macOS (or Windows/Linux) files | That platform's upload failed. Re-run **Build installers** for the tag; existing files are replaced. |
| No Release pull request appears | The last commits on `main` were only `ci:`, `docs:`, `chore:`, and so on. It appears after a `feat:` or `fix:`. Also check the *Allow GitHub Actions to create pull requests* setting. |
| The Release pull request has no **CI** check | It was opened with the default token. Add `RELEASE_PLEASE_TOKEN`, or merge it as an admin. |
| `Unexpected input(s) 'includeUpdaterJson'` | Harmless old warning. The valid input is `uploadUpdaterJson`. |
| `STATIC_VCRUNTIME is deprecated` in the Windows build | Harmless. The Tauri CLI still sets an environment variable that the newer build library prefers as a config option. It disappears when both are updated together. |
| Builds recompile every dependency | The Rust cache was not saved (a failed job saves nothing). `release.yml` uses `cache-on-failure`, so a retry starts warm. The first successful run after a `Cargo.lock` change is slower. |
| Pull request title check fails | Title must start with a type, for example `fix: handle empty selection`. |

## Known limits

- **macOS builds are ad-hoc signed and not notarized**, and Windows installers are not code-signed. Users see Gatekeeper and SmartScreen warnings, and macOS resets the Accessibility permission after each update. Real signing needs an Apple Developer ID and a Windows signing service (for open-source projects, SignPath Foundation is free); see [`../docs/RELEASING.md`](../docs/RELEASING.md).
- Only the Linux AppImage updates itself; `.deb` and `.rpm` users install the new file.
- The compile checks for macOS and Windows do not run `clippy` or the tests there.

## Possible next steps

- Signing and notarization for macOS and Windows (removes the warnings and the Accessibility reset).
- Package-manager publishing on release (Homebrew cask, winget).
- Scheduled `cargo audit`, `npm audit`, and CodeQL scans.
- Skipping Rust jobs on pull requests that only change docs or workflows.
