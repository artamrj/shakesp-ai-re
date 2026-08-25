<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
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
  let copyState = $state<"idle" | "copied" | "error">("idle");
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

  async function copyResult() {
    if (!outputText.trim()) return;
    try {
      await writeText(outputText);
      copyState = "copied";
      window.setTimeout(() => (copyState = "idle"), 1600);
    } catch {
      copyState = "error";
      window.setTimeout(() => (copyState = "idle"), 2000);
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
    } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "c") {
      event.preventDefault();
      void copyResult();
    }
  }
</script>

{#if isPopup}
  <main class="popup-shell">
    <header class="popup-header" data-tauri-drag-region>
      <div class="popup-title" data-tauri-drag-region>
        <strong data-tauri-drag-region>Proofread</strong>
      </div>
      {#if isStreaming}
        <span class="stream-status"><i></i>Working</span>
      {/if}
      <button class="icon-button" aria-label="Close" title="Close (Esc)" onclick={cancelPopup}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8" /></svg>
      </button>
    </header>

    <section class="result" class:loading={isStreaming} aria-live="polite" aria-busy={isStreaming}>
      {#if outputText}
        <p>{outputText}<span class:visible={isStreaming} class="cursor"></span></p>
      {:else if !popupError}
        <div class="skeleton" aria-label="Preparing your proofread result">
          <i></i><i></i><i></i>
        </div>
      {/if}
      {#if popupError}
        <div class="error" role="alert">
          <svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="8" /><path d="M10 6v5m0 3h.01" /></svg>
          <p>{popupError}</p>
        </div>
      {/if}
    </section>

    <footer class="popup-actions">
      <button class="primary" onclick={applyReplacement} disabled={isStreaming || isApplying || !outputText.trim()} title="Replace selected text (Enter)">
        {#if isApplying}<span class="button-spinner"></span>{/if}
        {isApplying ? "Replacing" : "Replace"}
      </button>
      <button class="secondary" onclick={copyResult} disabled={!outputText.trim()} title="Copy result (⌘C)">
        {copyState === "copied" ? "Copied" : copyState === "error" ? "Copy failed" : "Copy"}
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
    background: #f3f5f8;
    font-synthesis: none;
  }

  :global(*) { box-sizing: border-box; }
  :global(button), :global(input) { font: inherit; }
  :global(html.popup-view), :global(html.popup-view body), :global(html.popup-view #app) { background: transparent; }

  .settings-shell { max-width: 520px; margin: 0 auto; padding: 30px; }
  .brand { display: flex; align-items: center; gap: 13px; margin-bottom: 25px; }
  .brand-mark { color: #6857d9; }
  .brand-mark { display: grid; place-items: center; width: 42px; height: 42px; border-radius: 13px; background: #e7dcf8; font-size: 22px; }
  h1 { margin: 0; font-family: Georgia, serif; font-size: 26px; letter-spacing: -.4px; }
  .brand p { margin: 3px 0 0; color: #697184; font-size: 13px; }
  .config-card, .shortcut-card { border: 1px solid rgba(255,255,255,.9); border-radius: 18px; background: rgba(255,255,255,.72); box-shadow: 0 14px 40px rgba(35,43,70,.07); backdrop-filter: blur(18px); }
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

  .popup-shell { position: relative; display: flex; flex-direction: column; height: 100vh; overflow: hidden; border: 1px solid rgba(255,255,255,.22); border-radius: 16px; background: rgba(255,255,255,.025); box-shadow: inset 0 1px 0 rgba(255,255,255,.28); font-family: -apple-system, BlinkMacSystemFont, "Helvetica Neue", Arial, sans-serif; font-synthesis: none; font-weight: 300; -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale; }
  .popup-header { position: relative; z-index: 1; display: flex; align-items: center; flex: 0 0 auto; height: 36px; padding: 0 9px 0 14px; border-bottom: 1px solid rgba(87,94,108,.09); user-select: none; }
  .popup-title { display: flex; align-items: center; color: rgba(28,29,33,.80); font-size: 12px; font-weight: 400; letter-spacing: 0; }
  .popup-title strong { font-weight: 400; }
  .icon-button { display: grid; place-items: center; width: 22px; height: 22px; margin-left: 6px; padding: 0; border-radius: 50%; background: transparent; color: rgba(55,59,66,.48); opacity: 1; }
  .icon-button svg { width: 12px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-width: 1.4; }
  .icon-button:hover { background: rgba(60,64,72,.08) !important; color: rgba(35,38,44,.72); }
  .result { position: relative; z-index: 1; flex: 1 1 auto; min-height: 0; padding: 11px 15px 10px; overflow-y: auto; scrollbar-color: rgba(95,103,120,.16) transparent; scrollbar-width: thin; }
  .result > p { margin: 0; color: rgba(38,40,46,.78); font-family: -apple-system, BlinkMacSystemFont, "Helvetica Neue", Arial, sans-serif; font-size: 12px; font-style: normal; font-variation-settings: "wght" 300; font-weight: 300 !important; letter-spacing: .005em; line-height: 1.52; white-space: pre-wrap; }
  .stream-status { margin-left: auto; color: #7d828c; font-size: 9px; font-weight: 300; }
  .stream-status { display: flex; align-items: center; gap: 6px; }
  .stream-status i { width: 6px; height: 6px; border-radius: 50%; background: #7060df; box-shadow: 0 0 0 3px rgba(112,96,223,.10); animation: pulse 1s infinite alternate; }
  .cursor { display: inline-block; width: 2px; height: 1em; margin-left: 3px; border-radius: 2px; background: #6857d9; opacity: 0; vertical-align: -.12em; }
  .cursor.visible { opacity: 1; animation: blink .7s infinite; }
  .skeleton { display: grid; gap: 12px; padding-top: 3px; }
  .skeleton i { display: block; height: 10px; border-radius: 999px; background: linear-gradient(90deg, rgba(119,127,143,.13) 20%, rgba(119,127,143,.23) 40%, rgba(119,127,143,.13) 60%); background-size: 300% 100%; animation: shimmer 1.35s ease infinite; }
  .skeleton i:nth-child(2) { width: 92%; }
  .skeleton i:nth-child(3) { width: 54%; }
  .error { display: flex; align-items: flex-start; gap: 10px; padding: 12px 13px; border: 1px solid rgba(189,74,64,.13); border-radius: 11px; background: rgba(244,92,81,.08); color: #a43d35; }
  .error svg { flex: 0 0 auto; width: 18px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-width: 1.6; }
  .error p { margin: 0; font-size: 12px; font-weight: 300; line-height: 1.45; }
  .popup-actions { position: relative; z-index: 1; display: flex; align-items: center; gap: 6px; flex: 0 0 auto; min-height: 40px; padding: 6px 10px 8px; border-top: 1px solid rgba(87,94,108,.09); background: transparent; }
  .popup-actions button { min-width: 52px; padding: 5px 9px; border: 0; border-radius: 8px; font-size: 9px; font-weight: 400; box-shadow: inset 0 1px 0 rgba(255,255,255,.22); transition: transform .15s ease, background .15s ease; }
  .popup-actions button:active:not(:disabled) { transform: translateY(1px); }
  .popup-actions .primary { background: rgba(42,44,51,.84); color: rgba(255,255,255,.94); }
  .popup-actions .primary:hover:not(:disabled) { background: rgba(36,39,46,.92); }
  .popup-actions .secondary { border: 0; background: rgba(90,96,108,.10); color: rgba(35,38,44,.74); }
  .popup-actions .secondary:hover:not(:disabled) { background: rgba(90,96,108,.14); }
  .button-spinner { display: inline-block; width: 10px; height: 10px; margin-right: 5px; border: 1.5px solid rgba(255,255,255,.4); border-top-color: white; border-radius: 50%; animation: spin .7s linear infinite; }
  @keyframes pulse { to { opacity: .35; transform: scale(.8); } }
  @keyframes blink { 50% { opacity: 0; } }
  @keyframes shimmer { to { background-position: -150% 0; } }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .stream-status i, .cursor.visible, .skeleton i, .button-spinner { animation: none; } }
</style>
