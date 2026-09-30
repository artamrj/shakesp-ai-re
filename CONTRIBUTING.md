# Contributing to shakespAIre

Thanks for helping! Small fixes, bug reports, and ideas are all welcome.

## Getting started

You need Node.js (LTS), Rust (stable), and the
[Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/) for your system.

```bash
git clone https://github.com/artamrj/shakesp-ai-re.git
cd shakesp-ai-re
npm install
npm run tauri dev
```

If you also have the released app installed, use `npm run dev:side-by-side`. It gives the dev copy
its own settings so the two don't share a shortcut. Then set a different shortcut in the dev
copy's Settings.

## Before you open a pull request

Run the same checks CI runs:

```bash
npm run check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
cargo test --manifest-path src-tauri/Cargo.toml
```

CI also compiles the app on macOS and Windows, so platform-specific code is checked even if you
only have one system.

## Pull request rules

- **Title like a Conventional Commit:** `fix: …`, `feat: …`, `docs: …`, `ci: …`, `chore: …`. The
  title becomes the commit message, and the release bot reads it to choose the next version and
  write the changelog. CI checks it.
- **Do not change version numbers.** A bot opens a Release pull request that updates all of them
  together.
- **Keep changes focused.** One idea per pull request is easier to review and to release.
- **Keep Tauri versions matched.** The `tauri` crate and the `@tauri-apps/*` npm packages must
  share a minor version.
- **Never commit secrets.** The API key belongs in the keychain and signing keys in GitHub secrets.

## Naming

Write **shakespAIre** wherever people read it (UI, docs, messages), and **shakesp-ai-re** in
identifiers, file names, and URLs.

## Where things live

See the "Project layout" section of the [README](README.md). How CI and releases work is described
in [.github/CI.md](.github/CI.md).

## Good first issues

Look for the **good first issue** label. Improving platform support (Windows and Linux paths get
the least testing), documentation, and error messages are all useful places to start.

## Reporting bugs and ideas

Use the issue templates. For a bug, include your OS (and X11 or Wayland on Linux), the app version,
and the last lines of the log. **Never paste your API key.** For security problems, follow
[SECURITY.md](SECURITY.md).
