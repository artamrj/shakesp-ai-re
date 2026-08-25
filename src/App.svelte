<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";

  let config = $state({
    base_url: "https://api.openai.com/v1",
    api_key: "",
    model: "gpt-4o-mini",
  });
  let status = $state("Ready");
  let isTesting = $state(false);
  let testResult = $state("");
  let isDebugE2e = $state(false);
  let debugSource = $state<HTMLTextAreaElement>();

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];
    let disposed = false;

    async function setup() {
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

    setup().catch((error) => {
      if (!disposed) status = `Startup error: ${error}`;
    });

    return () => {
      disposed = true;
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
</script>

{#if isDebugE2e}
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
  .settings-shell { max-width: 520px; margin: 0 auto; padding: 30px; }
  .brand { display: flex; align-items: center; gap: 13px; margin-bottom: 25px; }
  .brand-mark { display: grid; place-items: center; width: 42px; height: 42px; border-radius: 13px; background: #e7dcf8; color: #6857d9; font-size: 22px; }
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
</style>
