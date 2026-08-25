# shakespAIre

A lightweight, cross-platform AI writing assistant for **Windows, macOS, and Linux**.

**AI for any selected text, anywhere, with one shortcut.**

Select text in any application, press a global shortcut, and a small floating popup streams an AI-improved version. Press **Enter** to replace the original text or **Esc** to cancel.

## Stack

- **Tauri 2 + Rust** — native OS functionality (global shortcuts, clipboard, text capture/replacement, window positioning)
- **Svelte 5 + TypeScript + Vite** — floating UI, config, actions, and streaming responses

## Status

M2 native flow implemented: the global shortcut captures selected text, restores the clipboard,
opens a frameless popup, streams an OpenAI-compatible chat completion, and supports Enter to
replace or Esc to cancel.

## Getting started

```bash
npm install
npm run tauri dev
```

On macOS, allow shakespAIre to control the computer under **System Settings → Privacy & Security
→ Accessibility**. Selection capture and replacement use native Cmd+C/Cmd+V events and will not
work until that permission is granted.

The API settings window accepts an OpenAI-compatible base URL, API key, and model. For local
development they can also be supplied as `OPENAI_BASE_URL`, `OPENAI_API_KEY`, and `OPENAI_MODEL`.

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).
