# shakespAIre

A lightweight, cross-platform AI writing assistant for **Windows, macOS, and Linux**.

**AI for any selected text, anywhere, with one shortcut.**

Select text in any application, press a global shortcut, and a small floating popup streams an AI-improved version. Press **Enter** to replace the original text or **Esc** to cancel.

## Stack

- **Tauri 2 + Rust** — native OS functionality (global shortcuts, clipboard, text capture/replacement, window positioning)
- **Svelte 5 + TypeScript + Vite** — floating UI, config, actions, and streaming responses

## Status

Early development. Current milestone: scaffolded project with a working build pipeline
(Frontend: Vite + Svelte 5 + TS · Backend: Tauri 2 + Rust with global-shortcut, clipboard-manager,
and opener plugins wired in).

## Getting started

```bash
npm install
npm run tauri dev
```

## Recommended IDE Setup

[VS Code](https://code.visualstudio.com/) + [Svelte](https://marketplace.visualstudio.com/items?itemName=svelte.svelte-vscode) + [Tauri](https://marketplace.visualstudio.com/items?itemName=tauri-apps.tauri-vscode) + [rust-analyzer](https://marketplace.visualstudio.com/items?itemName=rust-lang.rust-analyzer).
