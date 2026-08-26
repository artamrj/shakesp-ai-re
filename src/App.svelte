<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { onMount, tick } from "svelte";

  let config = $state({
    base_url: "https://api.openai.com/v1",
    api_key: "",
    model: "gpt-5.6-luna",
  });
  let status = $state("Ready");
  let isTesting = $state(false);
  let isTestingPopup = $state(false);
  let testResult = $state("");
  let isDebugE2e = $state(false);
  let debugSource = $state<HTMLTextAreaElement>();
  let shortcut = $state("CommandOrControl+Shift+Space");
  let isRecordingShortcut = $state(false);
  let isSavingShortcut = $state(false);
  let shortcutError = $state("");
  let recordingParts = $state<string[]>([]);
  const isMac = navigator.userAgent.includes("Mac");

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
        shortcut = await invoke<string>("get_shortcut");
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
      status = "Settings saved";
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

  async function testPopup() {
    isTestingPopup = true;
    try {
      await invoke("test_popup");
      status = "Test popup opened — popup display is working";
    } catch (error) {
      status = `Popup test failed: ${error}`;
    } finally {
      isTestingPopup = false;
    }
  }

  const supportedShortcutCodes = new Set([
    "Backquote", "Backslash", "BracketLeft", "BracketRight", "Comma", "Equal",
    "Minus", "Period", "Quote", "Semicolon", "Slash", "Space", "Tab", "Enter",
    "Backspace", "Delete", "End", "Home", "Insert", "PageDown", "PageUp",
    "ArrowDown", "ArrowLeft", "ArrowRight", "ArrowUp",
  ]);

  function isSupportedShortcutCode(code: string) {
    return supportedShortcutCodes.has(code) || /^(Key[A-Z]|Digit[0-9]|F(?:[1-9]|1[0-9]|2[0-4]))$/.test(code);
  }

  function shortcutParts(value: string) {
    const parts = value.split("+");
    const key = parts.pop();
    const modifierOrder = isMac
      ? ["super", "commandorcontrol", "control", "alt", "shift"]
      : ["commandorcontrol", "control", "alt", "shift", "super"];
    const displayParts = [
      ...parts.sort((a, b) => modifierOrder.indexOf(a.toLowerCase()) - modifierOrder.indexOf(b.toLowerCase())),
      ...(key ? [key] : []),
    ];

    return displayParts.map((part) => {
      const labels: Record<string, string> = isMac
        ? { super: "⌘", commandorcontrol: "⌘", control: "⌃", shift: "⇧", alt: "⌥" }
        : { super: "Win", commandorcontrol: "Ctrl", control: "Ctrl", shift: "Shift", alt: "Alt" };
      const normalized = part.toLowerCase();
      if (labels[normalized]) return labels[normalized];
      if (/^Key[A-Z]$/.test(part)) return part.slice(3);
      if (/^Digit[0-9]$/.test(part)) return part.slice(5);
      const keyLabels: Record<string, string> = {
        Space: "Space", ArrowUp: "↑", ArrowDown: "↓", ArrowLeft: "←", ArrowRight: "→",
        Backquote: "`", Backslash: "\\", BracketLeft: "[", BracketRight: "]",
        Comma: ",", Equal: "=", Minus: "−", Period: ".", Quote: "'", Semicolon: ";", Slash: "/",
        PageDown: "PgDn", PageUp: "PgUp",
      };
      return keyLabels[part] ?? part;
    });
  }

  function startShortcutRecording() {
    shortcutError = "";
    recordingParts = [];
    isRecordingShortcut = true;
  }

  function eventModifiers(event: KeyboardEvent) {
    return [
      event.ctrlKey && "Control",
      event.altKey && "Alt",
      event.shiftKey && "Shift",
      event.metaKey && "Super",
    ].filter((part): part is string => Boolean(part));
  }

  async function recordShortcut(event: KeyboardEvent) {
    if (!isRecordingShortcut) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat) return;

    if (event.key === "Escape") {
      isRecordingShortcut = false;
      recordingParts = [];
      shortcutError = "";
      return;
    }
    const modifiers = eventModifiers(event);
    recordingParts = modifiers;
    if (["Meta", "Control", "Alt", "Shift"].includes(event.key)) {
      shortcutError = "Keep holding the modifier, then press one other key.";
      return;
    }
    if (!isSupportedShortcutCode(event.code)) {
      shortcutError = "That key is not supported. Try a letter, number, arrow, or function key.";
      return;
    }

    if (modifiers.length === 0) {
      shortcutError = "Hold Command, Control, Option/Alt, or Shift while pressing the key.";
      return;
    }

    const candidate = [...modifiers, event.code].join("+");
    recordingParts = [...modifiers, event.code];
    isRecordingShortcut = false;
    isSavingShortcut = true;
    shortcutError = "";
    try {
      shortcut = await invoke<string>("set_shortcut", { shortcut: candidate });
      status = "Shortcut updated";
    } catch (error) {
      shortcutError = String(error);
      status = "Could not update shortcut";
      recordingParts = [];
      isRecordingShortcut = true;
    } finally {
      isSavingShortcut = false;
    }
  }
</script>

<svelte:window onkeydown={recordShortcut} />

{#if isDebugE2e}
  <main class="debug-e2e-shell">
    <textarea bind:this={debugSource}>This are a test sentence.</textarea>
  </main>
{:else}
  <main class="settings-shell">
    <div class="brand">
      <img class="brand-mark" src="/app-icon.png" alt="" />
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
        <input id="model" type="text" bind:value={config.model} placeholder="gpt-5.6-luna" />
      </div>
      <div class="settings-actions">
        <button onclick={saveConfig}>Save</button>
        <button class="secondary" onclick={testApi} disabled={isTesting}>{isTesting ? "Testing…" : "Test API"}</button>
        <button class="secondary" onclick={testPopup} disabled={isTestingPopup}>{isTestingPopup ? "Opening…" : "Test popup"}</button>
      </div>
      {#if testResult}<p class="test-result">{testResult}</p>{/if}
    </section>

    <section class="shortcut-card">
      <div class="shortcut-copy">
        <strong>Global shortcut</strong>
        <p>{isRecordingShortcut ? "Hold modifier(s) + one key. Esc cancels." : "Select text in another app, then press:"}</p>
      </div>
      <button
        class:recording={isRecordingShortcut}
        class="shortcut-recorder"
        type="button"
        aria-label="Customize global shortcut"
        aria-pressed={isRecordingShortcut}
        disabled={isSavingShortcut}
        onclick={startShortcutRecording}
      >
        {#if isRecordingShortcut}
          <span class="recording-dot"></span>
          {#if recordingParts.length > 0}
            {#each shortcutParts(recordingParts.join("+")) as part, index}
              {#if index > 0}<span class="plus">+</span>{/if}<kbd>{part}</kbd>
            {/each}
          {:else}
            <span>Press keys…</span>
          {/if}
        {:else}
          {#each shortcutParts(shortcut) as part, index}
            {#if index > 0}<span class="plus">+</span>{/if}<kbd>{part}</kbd>
          {/each}
        {/if}
      </button>
    </section>
    {#if shortcutError}<p class="shortcut-error">{shortcutError}</p>{/if}
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
  .brand-mark { display: block; width: 42px; height: 42px; border-radius: 12px; }
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
  .shortcut-copy { min-width: 0; }
  .shortcut-card strong { font-size: 13px; }
  .shortcut-card p { margin: 3px 0 0; color: #717889; font-size: 11px; }
  .shortcut-recorder { display: flex; min-width: 112px; min-height: 34px; align-items: center; justify-content: center; gap: 4px; padding: 5px 8px; border: 1px solid transparent; background: transparent; color: #8b8490; font-size: 10px; white-space: nowrap; }
  .shortcut-recorder:hover:not(:disabled) { border-color: #d8d3cb; background: #f8f5fb; }
  .shortcut-recorder:focus-visible { outline: none; border-color: #9068ce; box-shadow: 0 0 0 3px rgba(129,85,199,.13); }
  .shortcut-recorder.recording { border-color: #a98bd5; background: #f4eefb; color: #65429f; }
  .recording-dot { width: 7px; height: 7px; border-radius: 50%; background: #8155c7; box-shadow: 0 0 0 4px rgba(129,85,199,.12); }
  .plus { margin: 0 1px; color: #9a94a0; }
  kbd { display: inline-flex; min-width: 24px; justify-content: center; padding: 3px 6px; border: 1px solid #d8d3cb; border-radius: 6px; background: #faf9f7; box-shadow: 0 1px 0 #c9c4bc; color: #505767; font-family: inherit; font-size: 11px; }
  .shortcut-error { margin: 7px 4px 0; color: #a43f50; font-size: 11px; }
  .status { min-height: 18px; margin: 13px 0 0; color: #6d7485; text-align: center; font-size: 12px; }
  .debug-e2e-shell { display: grid; place-items: center; min-height: 100vh; padding: 30px; }
  .debug-e2e-shell textarea { width: 100%; min-height: 180px; }
</style>
