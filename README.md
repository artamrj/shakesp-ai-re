<div align="center">

<img src="assets/branding/shakespaire-icon-master.png" width="120" alt="shakespAIre" />

# shakespAIre

**Select. Shortcut. ShakespAIre it.**

Universal AI-powered proofreading for **macOS, Windows, and Linux**.

[![Latest release](https://img.shields.io/github/v/release/artamrj/shakesp-ai-re?label=release&color=blue)](https://github.com/artamrj/shakesp-ai-re/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)
[![CI](https://img.shields.io/github/actions/workflow/status/artamrj/shakesp-ai-re/release.yml?label=CI&logo=github)](https://github.com/artamrj/shakesp-ai-re/actions/workflows/release.yml)
[![Platforms](https://img.shields.io/badge/platforms-macOS%20%7C%20Windows%20%7C%20Linux-lightgrey)](#download)
[![Downloads](https://img.shields.io/github/downloads/artamrj/shakesp-ai-re/total?label=downloads&color=blue)](https://github.com/artamrj/shakesp-ai-re/releases)
[![Stars](https://img.shields.io/github/stars/artamrj/shakesp-ai-re?style=social)](https://github.com/artamrj/shakesp-ai-re/stargazers)

</div>

---

Select text in **any** application, press a global shortcut, and a small floating popup streams an AI-proofread version. Press **Enter** to replace the original text, **⌘/Ctrl+C** to copy, or **Esc** to cancel.

## Demo

<p align="center">
  <img src="assets/screenshots/Screen Recording 2026-08-26 at 16.47.49 (1).gif" width="640" alt="shakespAIre demo: select text, press the shortcut, stream the proofread, press Enter to replace" />
</p>

> **Add the demo:** record ~10 s — select a sentence with a typo in any app → press the shortcut → the popup streams a correction → press **Enter** → the text is replaced. Save as `assets/screenshots/demo.gif`.

## Features

- **Works anywhere** — email, docs, browsers, terminals, chat apps. Not bound to any single editor.
- **Strict proofreading** — fixes only grammar, spelling, and punctuation. Preserves your meaning, voice, tone, style, slang, dialect, emoji, and formatting.
- **Streaming results** — output streams in as it's generated, rendered as sanitized markdown.
- **Customizable shortcut** — record your own global shortcut from Settings; changes apply immediately and persist across launches.
- **OpenAI-compatible** — bring your own API key, or point it at a local model (Ollama, LM Studio, anything speaking the Chat Completions API).
- **Keychain-secured** — your API key lives in the OS keychain, never in a plain settings file.
- **Cross-platform native** — Tauri 2 + Rust + Svelte. No Electron, no browser bundle.
- **Retry & cancel** — transient failures retry automatically; you can cancel a stream mid-flight.

## How it works

1. **Select** text in any application.
2. **Press the shortcut** — shakespAIre captures your selection via the clipboard (then restores it), opens a frameless popup near the text (on macOS it uses the Accessibility API to position next to the selection).
3. **Stream** — the AI streams a proofread into the popup in real time.
4. **Act** — press **Enter** to replace the original, **⌘/Ctrl+C** to copy, or **Esc** to cancel.

## Download

Prebuilt installers for every release are on the [Releases page](https://github.com/artamrj/shakesp-ai-re/releases/latest).

| OS | Installer | Notes |
|---|---|---|
| **macOS** (Apple Silicon) | `.dmg` / `.app` | Grant **Accessibility** permission (see [Platform notes](#macos)) |
| **Windows** | NSIS `.exe` / `.msi` | Run at the same integrity level as the target app |
| **Linux** | AppImage / `.deb` / `.rpm` | Install [`wtype`](#linux) for Wayland or `xdotool` for X11 |

## Usage

### Keyboard shortcuts

| Action | Shortcut |
|---|---|
| Trigger a proofread | macOS **⇧⌘Space** · Windows/Linux **Shift+Ctrl+Space** |
| Replace the original text | **Enter** |
| Copy the result | **⌘/Ctrl+C** |
| Cancel | **Esc** |

The trigger shortcut is fully [customizable](#customizing-the-shortcut).

### The proofread popup

<p align="center">
  <img src="assets/screenshots/popup-streaming.png" width="360" alt="shakespAIre popup streaming a proofread" />
</p>

> **Add this screenshot:** trigger a proofread and capture it mid-stream — status pill reads "Writing", result text visible with a blinking cursor. Save as `assets/screenshots/popup-streaming.png`.

The popup shows a status pill while working (`Connecting` → `Writing` → `Ready`), renders the result as sanitized markdown, and exposes three actions: **Replace**, **Copy**, and **Close**. On a connection error it shows the message and a **Try again** button.

## Configuration

Open **Settings** (the main window) to configure the AI endpoint.

<p align="center">
  <img src="assets/screenshots/settings.png" width="420" alt="shakespAIre settings window" />
</p>

> **Add this screenshot:** capture the main/settings window showing the *OpenAI-compatible endpoint* card and the shortcut recorder. Save as `assets/screenshots/settings.png`.

### OpenAI-compatible endpoint

| Field | Default | Notes |
|---|---|---|
| **API base URL** | `https://api.openai.com/v1` | Any OpenAI-compatible endpoint |
| **API key** | *(empty)* | Optional for local servers; stored in the OS keychain |
| **Model** | `gpt-5.6-luna` | Any model name your endpoint accepts |

### Local models

Point shakespAIre at a local server to keep your text on your machine — no API key needed.

| Server | Base URL |
|---|---|
| [Ollama](https://ollama.com) | `http://localhost:11434/v1` |
| [LM Studio](https://lmstudio.ai) | `http://localhost:1234/v1` |

### Where secrets are stored

- **API key** → OS keychain (service `com.artamrj.shakespaire`, account `ai-api-key`). A plaintext file (`ai-api-key.txt`) is used only as a fallback if the keychain is unavailable.
- **Settings file** (`ai-settings.json`) stores the base URL, model, and shortcut — **never the API key** (enforced by a unit test).
- **Dev overrides:** `OPENAI_BASE_URL`, `OPENAI_API_KEY`, `OPENAI_MODEL` environment variables take precedence over the settings file.

### Diagnostics

- **Test API** — saves the config and sends a tiny request to verify the endpoint.
- **Test popup** — opens the popup in preview mode, bypassing the shortcut, selection capture, and AI call. Useful for checking that popup rendering works on a given device.

## Customizing the shortcut

<p align="center">
  <img src="assets/screenshots/shortcut-recorder.png" width="420" alt="shakespAIre shortcut recorder" />
</p>

> **Add this screenshot:** click the shortcut recorder so it shows "Hold modifier(s) + one key. Esc cancels.", then capture. Save as `assets/screenshots/shortcut-recorder.png`.

1. Open **Settings**.
2. Click the shortcut recorder — it prompts: *"Hold modifier(s) + one key. Esc cancels."*
3. Hold one or more modifiers (**⌘/Ctrl/Option/Shift**) and press a single key (letter, number, function key, arrow, or symbol).
4. The new shortcut is registered globally and takes effect immediately.

## Platform notes

<details>
<summary><strong>macOS</strong></summary>

- **Accessibility permission required.** Selection capture and replacement simulate **⌘C/⌘V** and read selection bounds via the Accessibility API. Grant it under **System Settings → Privacy & Security → Accessibility**.
- The popup positions itself next to the selected text (when the focused app exposes selection bounds).
- The window uses the native **Liquid Glass** effect (with a vibrancy fallback), so it is **direct-download only** — not available on the Mac App Store.
- Stable Developer ID signing with the unchanged bundle id `com.artamrj.shakespaire` reduces repeated Accessibility prompts.

</details>

<details>
<summary><strong>Windows</strong></summary>

- Text injection uses the native **`SendInput`** API and requires no helper program.
- It cannot inject into an application running with higher privileges (e.g. admin apps from a non-elevated install). Run both applications at the same integrity level.
- Unsigned installers trigger a Smart Screen warning until the build is signed with an EV/OV certificate.

</details>

<details>
<summary><strong>Linux</strong></summary>

- One input helper is required for copy/paste simulation (intentionally not bundled):
  - **Wayland:** install [`wtype`](https://github.com/atx/wtype) (preferred).
  - **X11:** install [`xdotool`](https://github.com/jordansissel/xdotool).
- On Wayland, `xdotool` is only a limited XWayland fallback and cannot control every native Wayland application.
- Selection-aware popup positioning is macOS-only; on Linux the popup appears near the cursor.

</details>

## Privacy & security

- **No telemetry, no analytics, no accounts.** shakespAIre doesn't phone home.
- Your **API key** is stored in the OS keychain — the settings file is verified to never contain it.
- Your **clipboard is saved and restored** around every capture and replace, so shakespAIre doesn't clobber what you copied.
- Your selected text is sent **only to the endpoint you configure**. Point it at a local model to keep text entirely on your machine.
- **Prompt-injection guard:** the selected text is treated as data, not as instructions to the model.

## Build from source

### Prerequisites

- **Node.js** LTS
- **Rust** (stable) — via [rustup](https://rustup.rs)
- Platform Tauri system dependencies — see the [Tauri 2 prerequisites](https://v2.tauri.app/start/prerequisites/).

### Commands

```bash
npm install          # install frontend deps
npm run tauri dev    # run a dev build
npm run check        # svelte-check (typecheck the frontend)
cargo test --manifest-path src-tauri/Cargo.toml   # run Rust tests
npm run bundle       # build all production bundles
# or platform-specific:
npm run bundle:macos    # .app + .dmg
npm run bundle:windows  # NSIS .exe + .msi
npm run bundle:linux    # AppImage + .deb + .rpm
```

Regenerate icons from the master source:

```bash
npm run icons        # tauri icon assets/branding/shakespaire-icon-master.png
```

## Releasing

Releases are produced by the [`release.yml`](.github/workflows/release.yml) workflow on native GitHub runners. Push a tag matching `app-v*` or `v*` to create a **draft** GitHub release with macOS, Windows, and Linux installers attached.

For signing, notarization, and the full release checklist, see [`docs/RELEASING.md`](docs/RELEASING.md).

## Contributing

Pull requests are welcome. Before submitting:

- Keep the version in sync across `src-tauri/tauri.conf.json`, `src-tauri/Cargo.toml`, and `package.json`.
- Run `npm run check` and `cargo test --manifest-path src-tauri/Cargo.toml` and make sure both pass.
- Don't commit secrets — the API key belongs in the keychain, not in a config file.

## License

[MIT](LICENSE) © 2026 Arta
