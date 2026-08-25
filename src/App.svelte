<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";

  const isPopup =
    window.location.pathname === "/popup" ||
    new URLSearchParams(window.location.search).get("view") === "popup";

  let config = $state({
    base_url: "https://api.openai.com/v1",
    api_key: "",
    model: "gpt-4o-mini",
  });
  let status = $state("Ready");
  let isTesting = $state(false);
  let testResult = $state("");

  let selectedText = $state("");
  let outputText = $state("");
  let popupError = $state("");
  let isStreaming = $state(false);
  let isApplying = $state(false);
  let isDebugE2e = $state(false);
  let debugSource = $state<HTMLTextAreaElement>();

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];
    let disposed = false;

    async function setup() {
      if (isPopup) {
        unlisteners.push(
          await listen<string>("ai-stream-chunk", (event) => {
            outputText += event.payload;
          }),
          await listen<string>("ai-stream-error", (event) => {
            popupError = event.payload;
            isStreaming = false;
          }),
          await listen("ai-stream-done", () => {
            isStreaming = false;
            void invoke("debug_e2e_report", {
              selectedText,
              outputText,
              error: popupError,
            }).catch(() => {});
          }),
        );

        selectedText = await invoke<string>("get_popup_selection");
        if (!selectedText.trim()) {
          popupError = "No selected text was captured.";
          return;
        }

        isStreaming = true;
        invoke("stream_ai_text", { selectedText }).catch((error) => {
          popupError = String(error);
          isStreaming = false;
        });
        window.addEventListener("keydown", handlePopupKeydown);
      } else {
        isDebugE2e = await invoke<boolean>("debug_e2e_enabled").catch(() => false);
        if (isDebugE2e) {
          await tick();
          debugSource?.focus();
          debugSource?.select();
          await new Promise((resolve) => window.setTimeout(resolve, 300));
          await invoke("debug_trigger_shortcut");
          return;
        }

        try {
          config = await invoke<typeof config>("get_ai_config");
        } catch (error) {
          status = `Could not load settings: ${error}`;
        }

        unlisteners.push(
          await listen("shortcut-triggered", () => {
            status = "Selection captured — popup opened";
          }),
          await listen<string>("shortcut-error", (event) => {
            status = `Shortcut error: ${event.payload}`;
          }),
        );
      }
    }

    setup().catch((error) => {
      if (!disposed) {
        if (isPopup) popupError = String(error);
        else status = `Startup error: ${error}`;
      }
    });

    return () => {
      disposed = true;
      window.removeEventListener("keydown", handlePopupKeydown);
      for (const unlisten of unlisteners) unlisten();
    };
  });

  async function saveConfig() {
    try {
      await invoke("set_ai_config", { config });
      status = "Settings saved for this session";
    } catch (error) {
      status = `Error saving settings: ${error}`;
    }
  }

  async function testApi() {
    isTesting = true;
    testResult = "Testing…";
    try {
      await invoke("set_ai_config", { config });
      await invoke("stream_ai_text", { selectedText: "Reply with the word ready." });
      testResult = "Connection succeeded.";
    } catch (error) {
      testResult = `Connection failed: ${error}`;
    } finally {
      isTesting = false;
    }
  }

  async function applyReplacement() {
    if (isStreaming || isApplying || !outputText.trim()) return;
    isApplying = true;
    popupError = "";
    try {
      await invoke("replace_text", { text: outputText });
    } catch (error) {
      popupError = String(error);
      isApplying = false;
    }
  }

  async function cancelPopup() {
    await invoke("close_popup");
  }

  function handlePopupKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      void cancelPopup();
    } else if (event.key === "Enter" && !event.shiftKey && !event.metaKey && !event.ctrlKey) {
      event.preventDefault();
      void applyReplacement();
    }
  }
</script>

{#if isPopup}
  <main class="popup-shell">
    <header class="popup-header" data-tauri-drag-region>
      <div data-tauri-drag-region>
        <span class="spark">✦</span>
        <strong>shakespAIre</strong>
      </div>
      <button class="icon-button" aria-label="Close" title="Close (Esc)" onclick={cancelPopup}>×</button>
    </header>

    <section class="selection" aria-label="Selected text">
      <span>Original</span>
      <p>{selectedText}</p>
    </section>

    <section class="result" class:loading={isStreaming} aria-live="polite" aria-busy={isStreaming}>
      <div class="result-label">
        <span>Rewrite</span>
        {#if isStreaming}<span class="stream-status"><i></i> Writing…</span>{/if}
      </div>
      {#if outputText}
        <p>{outputText}<span class:visible={isStreaming} class="cursor"></span></p>
      {:else if !popupError}
        <p class="placeholder">Waiting for the first words…</p>
      {/if}
      {#if popupError}<p class="error">{popupError}</p>{/if}
    </section>

    <footer class="popup-actions">
      <span><kbd>Esc</kbd> cancel</span>
      <button class="secondary" onclick={cancelPopup}>Cancel</button>
      <button onclick={applyReplacement} disabled={isStreaming || isApplying || !outputText.trim()}>
        {isApplying ? "Applying…" : "Replace"} <kbd>↵</kbd>
      </button>
    </footer>
  </main>
{:else if isDebugE2e}
  <main class="debug-e2e-shell">
    <textarea bind:this={debugSource}>This are a test sentence.</textarea>
  </main>
{:else}
  <main class="settings-shell">
    <div class="brand">
      <span class="brand-mark">✦</span>
      <div>
        <h1>shakespAIre</h1>
        <p>AI for any selected text, anywhere.</p>
      </div>
    </div>

    <section class="config-card">
      <h2>OpenAI-compatible endpoint</h2>
      <div class="field">
        <label for="base_url">API base URL</label>
        <input id="base_url" type="url" bind:value={config.base_url} placeholder="https://api.openai.com/v1" />
      </div>
      <div class="field">
        <label for="api_key">API key</label>
        <input id="api_key" type="password" bind:value={config.api_key} placeholder="sk-… (optional for local servers)" />
      </div>
      <div class="field">
        <label for="model">Model</label>
        <input id="model" type="text" bind:value={config.model} placeholder="gpt-4o-mini" />
      </div>
      <div class="settings-actions">
        <button onclick={saveConfig}>Save</button>
        <button class="secondary" onclick={testApi} disabled={isTesting}>{isTesting ? "Testing…" : "Test API"}</button>
      </div>
      {#if testResult}<p class="test-result">{testResult}</p>{/if}
    </section>

    <section class="shortcut-card">
      <div>
        <strong>Global shortcut</strong>
        <p>Select text in another app, then press:</p>
      </div>
      <div class="shortcut"><kbd>⌘</kbd><span>+</span><kbd>⇧</kbd><span>+</span><kbd>Space</kbd></div>
    </section>
    <p class="status">{status}</p>
  </main>
{/if}

<style>
  :global(:root) {
    font-family: Inter, ui-sans-serif, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    color: #172033;
    background: #f4f1eb;
    font-synthesis: none;
  }

  :global(*) { box-sizing: border-box; }
  :global(button), :global(input) { font: inherit; }

  .settings-shell { max-width: 520px; margin: 0 auto; padding: 30px; }
  .brand { display: flex; align-items: center; gap: 13px; margin-bottom: 25px; }
  .brand-mark, .spark { color: #8155c7; }
  .brand-mark { display: grid; place-items: center; width: 42px; height: 42px; border-radius: 13px; background: #e7dcf8; font-size: 22px; }
  h1 { margin: 0; font-family: Georgia, serif; font-size: 26px; letter-spacing: -.4px; }
  .brand p { margin: 3px 0 0; color: #697184; font-size: 13px; }
  .config-card, .shortcut-card { border: 1px solid #ded9cf; border-radius: 14px; background: rgba(255,255,255,.8); box-shadow: 0 10px 30px rgba(48,39,27,.05); }
  .config-card { padding: 20px; }
  h2 { margin: 0 0 17px; font-size: 15px; }
  .field { margin-bottom: 13px; }
  label { display: block; margin-bottom: 5px; color: #535b6c; font-size: 12px; font-weight: 650; }
  input { width: 100%; padding: 10px 11px; border: 1px solid #d5d1c9; border-radius: 9px; color: #172033; background: white; outline: none; }
  input:focus { border-color: #9068ce; box-shadow: 0 0 0 3px rgba(129,85,199,.13); }
  button { border: 0; border-radius: 9px; padding: 9px 14px; background: #7550b5; color: white; font-weight: 650; cursor: pointer; }
  button:hover:not(:disabled) { background: #65429f; }
  button:disabled { opacity: .48; cursor: default; }
  button.secondary { border: 1px solid #d8d3cb; background: white; color: #4a5262; }
  button.secondary:hover:not(:disabled) { background: #f6f3ee; }
  .settings-actions { display: flex; gap: 8px; margin-top: 18px; }
  .test-result { margin: 12px 0 0; color: #596174; font-size: 12px; }
  .shortcut-card { display: flex; justify-content: space-between; align-items: center; gap: 12px; margin-top: 14px; padding: 15px 18px; }
  .shortcut-card strong { font-size: 13px; }
  .shortcut-card p { margin: 3px 0 0; color: #717889; font-size: 11px; }
  .shortcut { display: flex; align-items: center; gap: 4px; color: #8b8490; font-size: 10px; white-space: nowrap; }
  kbd { display: inline-flex; min-width: 24px; justify-content: center; padding: 3px 6px; border: 1px solid #d8d3cb; border-radius: 6px; background: #faf9f7; box-shadow: 0 1px 0 #c9c4bc; color: #505767; font-family: inherit; font-size: 11px; }
  .status { min-height: 18px; margin: 13px 0 0; color: #6d7485; text-align: center; font-size: 12px; }
  .debug-e2e-shell { display: grid; place-items: center; min-height: 100vh; padding: 30px; }
  .debug-e2e-shell textarea { width: 100%; min-height: 180px; }

  .popup-shell { display: flex; flex-direction: column; height: 100vh; overflow: hidden; border: 1px solid #d8d2c8; background: #fbfaf7; box-shadow: inset 0 1px rgba(255,255,255,.8); }
  .popup-header { display: flex; justify-content: space-between; align-items: center; flex: 0 0 auto; height: 45px; padding: 0 10px 0 16px; border-bottom: 1px solid #e5e0d8; background: #f5f1eb; user-select: none; }
  .popup-header > div { display: flex; align-items: center; gap: 7px; font-family: Georgia, serif; font-size: 14px; }
  .icon-button { display: grid; place-items: center; width: 28px; height: 28px; padding: 0; border-radius: 7px; background: transparent; color: #757b88; font-size: 21px; font-weight: 400; }
  .icon-button:hover { background: #e9e4dc !important; color: #303746; }
  .selection { flex: 0 0 auto; max-height: 82px; padding: 11px 16px 12px; border-bottom: 1px solid #e7e2da; background: #f8f6f2; overflow: hidden; }
  .selection span, .result-label > span:first-child { color: #858b98; font-size: 10px; font-weight: 750; letter-spacing: .08em; text-transform: uppercase; }
  .selection p { margin: 5px 0 0; overflow: hidden; color: #656d7b; font-size: 12px; line-height: 1.4; text-overflow: ellipsis; white-space: nowrap; }
  .result { flex: 1 1 auto; min-height: 0; padding: 15px 17px; overflow-y: auto; }
  .result-label { display: flex; justify-content: space-between; align-items: center; }
  .result p { margin: 10px 0; color: #202838; font-family: Georgia, serif; font-size: 16px; line-height: 1.55; white-space: pre-wrap; }
  .result p.placeholder { color: #a0a4ae; font-family: inherit; font-size: 13px; }
  .result p.error { padding: 10px 11px; border-radius: 8px; background: #fae8e5; color: #a43d31; font-family: inherit; font-size: 12px; line-height: 1.4; }
  .stream-status { display: flex; align-items: center; gap: 5px; color: #8060ae; font-size: 11px; }
  .stream-status i { width: 6px; height: 6px; border-radius: 50%; background: #9466d1; animation: pulse 1s infinite alternate; }
  .cursor { display: inline-block; width: 2px; height: 1em; margin-left: 2px; background: #8155c7; opacity: 0; vertical-align: -.12em; }
  .cursor.visible { opacity: 1; animation: blink .7s infinite; }
  .popup-actions { display: flex; align-items: center; justify-content: flex-end; gap: 8px; flex: 0 0 auto; padding: 10px 13px; border-top: 1px solid #e5e0d8; background: #f5f1eb; }
  .popup-actions > span { margin-right: auto; color: #888e9a; font-size: 10px; }
  .popup-actions button { padding: 7px 11px; font-size: 12px; }
  .popup-actions button kbd { margin-left: 5px; padding: 1px 4px; border-color: rgba(255,255,255,.35); background: rgba(255,255,255,.14); box-shadow: none; color: white; }
  @keyframes pulse { to { opacity: .35; transform: scale(.8); } }
  @keyframes blink { 50% { opacity: 0; } }
</style>
