<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { onMount, tick } from "svelte";
  import { eventModifiers, isSupportedShortcutCode, shortcutParts as formatParts } from "./shortcut";

  let config = $state({
    base_url: "https://api.openai.com/v1",
    api_key: "",
    model: "gpt-5.6-luna",
  });
  // Feedback lives next to the thing it is about, so there is no global status line.
  let notes = $state({ connection: "", shortcut: "" });
  let saveState = $state<"idle" | "busy" | "done">("idle");
  let saveTimer: number | undefined;
  let testState = $state<"idle" | "busy" | "ok" | "fail">("idle");
  let isDebugE2e = $state(false);
  let debugSource = $state<HTMLTextAreaElement>();
  let shortcut = $state("CommandOrControl+Shift+Space");
  let replaceShortcut = $state("Enter");
  let recordingTarget = $state<"trigger" | "replace" | null>(null);
  let isSavingShortcut = $state(false);
  let recordingParts = $state<string[]>([]);
  let accessibilityOk = $state(true);
  let launchAtLogin = $state(false);
  let isTogglingLogin = $state(false);
  const isMac = navigator.userAgent.includes("Mac");

  const shortcutParts = (value: string) => formatParts(value, isMac);

  function note(area: "connection" | "shortcut", text = "") {
    notes[area] = text;
  }

  async function refreshAccessibility() {
    if (!isMac) return;
    accessibilityOk = await invoke<boolean>("accessibility_trusted").catch(() => true);
  }

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
        replaceShortcut = await invoke<string>("get_replace_shortcut");
      } catch (error) {
        note("connection", `Could not load settings: ${error}`);
      }
      await refreshAccessibility();
      launchAtLogin = await isEnabled().catch(() => false);

      unlisteners.push(
        await listen<string>("shortcut-error", (event) => {
          note("shortcut", event.payload);
          void refreshAccessibility();
        }),
        // Re-check permission when the user comes back from System Settings.
        await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
          if (focused) void refreshAccessibility();
        }),
      );
    }

    setup().catch((error) => {
      if (!disposed) note("connection", `Startup error: ${error}`);
    });

    return () => {
      disposed = true;
      for (const unlisten of unlisteners) unlisten();
    };
  });

  async function saveConfig() {
    if (saveState === "busy") return;
    saveState = "busy";
    note("connection");
    try {
      await invoke("set_ai_config", { config });
      saveState = "done";
      window.clearTimeout(saveTimer);
      saveTimer = window.setTimeout(() => (saveState = "idle"), 1800);
    } catch (error) {
      saveState = "idle";
      note("connection", String(error));
    }
  }

  async function testConnection() {
    if (testState === "busy") return;
    testState = "busy";
    note("connection");
    try {
      await invoke("test_ai_connection", { config });
      testState = "ok";
    } catch (error) {
      testState = "fail";
      note("connection", String(error));
    }
  }

  // A previous test result no longer applies once the form changes.
  $effect(() => {
    config.base_url;
    config.api_key;
    config.model;
    testState = "idle";
    notes.connection = "";
  });

  async function toggleLaunchAtLogin() {
    if (isTogglingLogin) return;
    isTogglingLogin = true;
    note("shortcut");
    try {
      if (launchAtLogin) await disable();
      else await enable();
      launchAtLogin = await isEnabled();
    } catch (error) {
      note("shortcut", `Could not change launch at login: ${error}`);
    } finally {
      isTogglingLogin = false;
    }
  }

  async function previewPopup() {
    try {
      await invoke("test_popup");
    } catch (error) {
      note("shortcut", `Popup preview failed: ${error}`);
    }
  }

  function openAccessibilitySettings() {
    void invoke("open_accessibility_settings");
  }

  function startRecording(target: "trigger" | "replace") {
    note("shortcut");
    recordingParts = [];
    recordingTarget = target;
  }

  async function recordShortcut(event: KeyboardEvent) {
    const target = recordingTarget;
    if (!target) return;
    event.preventDefault();
    event.stopPropagation();
    if (event.repeat) return;

    if (event.key === "Escape") {
      recordingTarget = null;
      recordingParts = [];
      note("shortcut");
      return;
    }
    const modifiers = eventModifiers(event);
    recordingParts = modifiers;
    if (["Meta", "Control", "Alt", "Shift"].includes(event.key)) {
      note("shortcut", "Keep holding the modifier, then press one other key.");
      return;
    }
    if (!isSupportedShortcutCode(event.code)) {
      note("shortcut", "That key is not supported. Try a letter, number, arrow, or function key.");
      return;
    }
    // The global trigger needs a modifier; the in-popup replace key can be a single key.
    if (target === "trigger" && modifiers.length === 0) {
      note("shortcut", "Hold Command, Control, Option/Alt, or Shift while pressing the key.");
      return;
    }

    const candidate = [...modifiers, event.code].join("+");
    recordingParts = [...modifiers, event.code];
    recordingTarget = null;
    isSavingShortcut = true;
    note("shortcut");
    try {
      if (target === "trigger") {
        shortcut = await invoke<string>("set_shortcut", { shortcut: candidate });
      } else {
        replaceShortcut = await invoke<string>("set_replace_shortcut", { shortcut: candidate });
      }
    } catch (error) {
      note("shortcut", String(error));
      recordingParts = [];
      recordingTarget = target;
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
  <main class="shell">
    <form class="panel" onsubmit={(event) => { event.preventDefault(); void saveConfig(); }}>
      <label class="row">
        <span>Server</span>
        <input type="url" bind:value={config.base_url} placeholder="https://api.openai.com/v1" spellcheck="false" autocapitalize="off" autocomplete="off" />
      </label>
      <label class="row">
        <span>API key</span>
        <input type="password" bind:value={config.api_key} placeholder="Optional for local models" spellcheck="false" autocomplete="off" />
      </label>
      <label class="row">
        <span>Model</span>
        <input type="text" bind:value={config.model} placeholder="gpt-5.6-luna" spellcheck="false" autocapitalize="off" autocomplete="off" />
      </label>
      <div class="row">
        <span>Connection</span>
        {#if testState === "busy"}
          <span class="status"><i class="spinner"></i>Testing…</span>
        {:else if testState === "ok"}
          <button class="status ok" type="button" onclick={testConnection} title="Test again">
            <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2.5 6.2 2.2 2.2 4.8-5" /></svg>Connected
          </button>
        {:else if testState === "fail"}
          <button class="status bad" type="button" onclick={testConnection}>Failed · Retry</button>
        {:else}
          <button class="link" type="button" onclick={testConnection}>Test connection</button>
        {/if}
      </div>
      {#if notes.connection}<p class="row note" role="alert">{notes.connection}</p>{/if}
      <button type="submit" hidden aria-hidden="true" tabindex="-1"></button>
    </form>

    <div class="panel">
      {#each [{ target: "trigger", label: "Shortcut", value: shortcut }, { target: "replace", label: "Replace key", value: replaceShortcut }] as item (item.target)}
        <div class="row">
          <span>{item.label}</span>
          <button
            class="recorder"
            class:recording={recordingTarget === item.target}
            type="button"
            aria-label="Change {item.label.toLowerCase()}"
            aria-pressed={recordingTarget === item.target}
            disabled={isSavingShortcut}
            onclick={() => startRecording(item.target as "trigger" | "replace")}
          >
            {#if recordingTarget === item.target}
              {#if recordingParts.length > 0}
                {#each shortcutParts(recordingParts.join("+")) as part}<kbd>{part}</kbd>{/each}
              {:else}
                <em>Press keys…</em>
              {/if}
            {:else}
              {#each shortcutParts(item.value) as part}<kbd>{part}</kbd>{/each}
            {/if}
          </button>
        </div>
      {/each}
      <div class="row">
        <span>Launch at login</span>
        <button class="switch" class:on={launchAtLogin} type="button" role="switch" aria-checked={launchAtLogin} aria-label="Launch at login" disabled={isTogglingLogin} onclick={toggleLaunchAtLogin}><i></i></button>
      </div>
      {#if notes.shortcut}<p class="row note" role="alert">{notes.shortcut}</p>{/if}
      {#if isMac}
        <div class="row">
          <span>Accessibility</span>
          {#if accessibilityOk}
            <span class="badge ok">Allowed</span>
          {:else}
            <button class="link" type="button" onclick={openAccessibilitySettings}>Allow in Settings…</button>
          {/if}
        </div>
      {/if}
    </div>

    <div class="footer">
      <button class="btn btn-primary save" type="button" onclick={saveConfig} disabled={saveState === "busy"}>
        {#if saveState === "busy"}<i class="spinner light"></i>Saving…
        {:else if saveState === "done"}<svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2.5 6.2 2.2 2.2 4.8-5" /></svg>Saved
        {:else}Save{/if}
      </button>
      <button class="link quiet" type="button" onclick={previewPopup}>Preview popup</button>
    </div>
  </main>
{/if}

<style>
  .shell { display: flex; flex-direction: column; gap: 14px; height: 100vh; padding: 20px 20px 16px; user-select: none; }

  .panel { flex: none; margin: 0; overflow: hidden; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 46px; padding: 0 14px; }
  .row + .row { border-top: 1px solid var(--separator); }
  .row > span:first-child { flex: none; min-width: 88px; }
  .row input { flex: 1; min-width: 0; height: 30px; padding: 0 10px; border: 1px solid var(--field-edge); border-radius: 8px; outline: 0; background: var(--field); user-select: text; text-overflow: ellipsis; transition: border-color 0.15s ease, box-shadow 0.15s ease; }
  .row input::placeholder { color: var(--secondary); opacity: 0.7; }
  .row input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }

  .recorder { display: flex; align-items: center; gap: 4px; min-height: 26px; padding: 0 6px; border: 0; border-radius: 8px; background: transparent; cursor: pointer; }
  .recorder:hover:not(:disabled) { background: var(--key); }
  .recorder.recording { background: color-mix(in srgb, var(--accent) 14%, transparent); color: var(--accent); }
  .recorder em { font-style: normal; font-size: 12px; }
  .switch { position: relative; flex: none; width: 40px; height: 24px; padding: 0; border: 0; border-radius: 999px; background: var(--key); cursor: pointer; transition: background 0.2s ease; }
  .switch i { position: absolute; top: 2px; left: 2px; width: 20px; height: 20px; border-radius: 50%; background: #fff; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.3); transition: transform 0.2s ease; }
  .switch.on { background: var(--accent); }
  .switch.on i { transform: translateX(16px); }
  .switch:disabled { opacity: 0.6; cursor: default; }
  .badge.ok { color: var(--success); font-weight: 500; }

  .footer { display: flex; flex-direction: column; align-items: center; gap: 8px; margin-top: auto; }
  .save { width: 100%; }
  .status { display: inline-flex; align-items: center; gap: 6px; padding: 0; border: 0; background: none; font-size: 13px; }
  button.status { cursor: pointer; }
  .status.ok { color: var(--success); font-weight: 500; }
  .status.bad { color: var(--danger); font-weight: 500; }
  .status svg, .save svg { width: 12px; height: 12px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.8; }
  .spinner { display: inline-block; width: 12px; height: 12px; border: 1.5px solid color-mix(in srgb, var(--secondary) 40%, transparent); border-top-color: var(--secondary); border-radius: 50%; animation: spin 0.7s linear infinite; }
  .spinner.light { border-color: rgba(255, 255, 255, 0.4); border-top-color: #fff; }
  @keyframes spin { to { transform: rotate(360deg); } }
  .note { margin: 0; padding-block: 8px; color: var(--danger); font-size: 12px; line-height: 16px; user-select: text; display: block; min-height: 0; }
  @media (prefers-reduced-motion: reduce) { .spinner, .switch, .switch i { animation: none; transition: none; } }
  .quiet { color: var(--secondary); font-size: 12px; }

  .debug-e2e-shell { display: grid; place-items: center; min-height: 100vh; padding: 30px; }
  .debug-e2e-shell textarea { width: 100%; min-height: 180px; }
</style>
