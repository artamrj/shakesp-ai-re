<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { getCurrentWindow } from "@tauri-apps/api/window";
  import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
  import { onMount, tick } from "svelte";
  import { eventModifiers, isSupportedShortcutCode, shortcutParts as formatParts } from "./shortcut";
  import { findLanguage, searchLanguages, type Language } from "./languages";

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
  type RecordingTarget = "trigger" | "replace" | "translate";
  let recordingTarget = $state<RecordingTarget | null>(null);
  let isSavingShortcut = $state(false);
  let recordingParts = $state<string[]>([]);
  let accessibilityOk = $state(true);
  let launchAtLogin = $state(false);
  let isTogglingLogin = $state(false);
  let showChanges = $state(false);
  let isTogglingChanges = $state(false);
  let changesNote = $state("");
  const isMac = navigator.userAgent.includes("Mac");

  let tab = $state<"general" | "translation">("general");
  let translation = $state({ enabled: false, language: "English", shortcut: isMac ? "Shift+Super+KeyY" : "Shift+Control+KeyY" });
  let isSavingTranslation = $state(false);
  let translationNote = $state("");
  let pickerOpen = $state(false);
  let languageQuery = $state("");
  let highlighted = $state(0);
  let languageSearch = $state<HTMLInputElement>();
  let languageList = $state<HTMLElement>();
  const currentLanguage = $derived(findLanguage(translation.language));
  const languageMatches = $derived(searchLanguages(languageQuery));

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
        showChanges = await invoke<boolean>("get_show_changes");
        translation = await invoke<typeof translation>("get_translation_settings");
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

  async function toggleShowChanges() {
    if (isTogglingChanges) return;
    isTogglingChanges = true;
    changesNote = "";
    const enabled = !showChanges;
    try {
      await invoke("set_show_changes", { enabled });
      showChanges = enabled;
    } catch (error) {
      changesNote = String(error);
    } finally {
      isTogglingChanges = false;
    }
  }

  function openAccessibilitySettings() {
    void invoke("open_accessibility_settings");
  }

  /** Saves the translator section; every change applies immediately, with rollback on failure. */
  async function saveTranslation(changes: Partial<typeof translation>) {
    if (isSavingTranslation) return false;
    isSavingTranslation = true;
    translationNote = "";
    try {
      translation = await invoke<typeof translation>("set_translation_settings", {
        settings: { ...translation, ...changes },
      });
      return true;
    } catch (error) {
      translationNote = String(error);
      return false;
    } finally {
      isSavingTranslation = false;
    }
  }

  async function openPicker() {
    if (pickerOpen) return closePicker();
    languageQuery = "";
    highlighted = Math.max(0, languageMatches.findIndex((language) => language.name === translation.language));
    pickerOpen = true;
    await tick();
    languageSearch?.focus();
    scrollHighlightedIntoView();
  }

  function closePicker() {
    pickerOpen = false;
    languageQuery = "";
  }

  async function chooseLanguage(language: Language) {
    closePicker();
    if (language.name !== translation.language) await saveTranslation({ language: language.name });
  }

  function scrollHighlightedIntoView() {
    void tick().then(() => languageList?.querySelector<HTMLElement>(`[data-index="${highlighted}"]`)?.scrollIntoView({ block: "nearest" }));
  }

  function pickerKeydown(event: KeyboardEvent) {
    if (event.isComposing) return;
    const last = languageMatches.length - 1;
    if (event.key === "ArrowDown" || event.key === "ArrowUp") {
      event.preventDefault();
      if (last < 0) return;
      highlighted = event.key === "ArrowDown" ? (highlighted >= last ? 0 : highlighted + 1) : (highlighted <= 0 ? last : highlighted - 1);
      scrollHighlightedIntoView();
    } else if (event.key === "Enter") {
      event.preventDefault();
      const language = languageMatches[highlighted];
      if (language) void chooseLanguage(language);
    } else if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      closePicker();
    }
  }

  $effect(() => {
    languageQuery;
    highlighted = 0;
  });

  async function previewTranslation() {
    try {
      await invoke("test_popup", { translate: true });
    } catch (error) {
      translationNote = `Popup preview failed: ${error}`;
    }
  }

  function startRecording(target: RecordingTarget) {
    note("shortcut");
    translationNote = "";
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
    const say = (text: string) => (target === "translate" ? (translationNote = text) : note("shortcut", text));
    const modifiers = eventModifiers(event);
    recordingParts = modifiers;
    if (["Meta", "Control", "Alt", "Shift"].includes(event.key)) {
      say("Keep holding the modifier, then press one other key.");
      return;
    }
    if (!isSupportedShortcutCode(event.code)) {
      say("That key is not supported. Try a letter, number, arrow, or function key.");
      return;
    }
    // The global trigger needs a modifier; the in-popup replace key can be a single key.
    if (target !== "replace" && modifiers.length === 0) {
      say("Hold Command, Control, Option/Alt, or Shift while pressing the key.");
      return;
    }

    const candidate = [...modifiers, event.code].join("+");
    recordingParts = [...modifiers, event.code];
    recordingTarget = null;
    isSavingShortcut = true;
    note("shortcut");
    translationNote = "";
    if (target === "translate") {
      const saved = await saveTranslation({ shortcut: candidate });
      isSavingShortcut = false;
      recordingParts = [];
      if (!saved) recordingTarget = target;
      return;
    }
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
  {#snippet recorder(target: RecordingTarget, label: string, value: string, disabled = false)}
    <div class="row">
      <span>{label}</span>
      <button
        class="recorder"
        class:recording={recordingTarget === target}
        type="button"
        aria-label="Change {label.toLowerCase()}"
        aria-pressed={recordingTarget === target}
        disabled={isSavingShortcut || disabled}
        onclick={() => startRecording(target)}
      >
        {#if recordingTarget === target}
          {#if recordingParts.length > 0}
            {#each shortcutParts(recordingParts.join("+")) as part}<kbd>{part}</kbd>{/each}
          {:else}
            <em>Press keys…</em>
          {/if}
        {:else}
          {#each shortcutParts(value) as part}<kbd>{part}</kbd>{/each}
        {/if}
      </button>
    </div>
  {/snippet}

  <main class="shell">
    <div class="tabs" role="tablist" aria-label="Settings sections">
      <button type="button" role="tab" id="tab-general" aria-selected={tab === "general"} aria-controls="panel-general" class:active={tab === "general"} onclick={() => { tab = "general"; recordingTarget = null; closePicker(); }}>General</button>
      <button type="button" role="tab" id="tab-translation" aria-selected={tab === "translation"} aria-controls="panel-translation" class:active={tab === "translation"} onclick={() => { tab = "translation"; recordingTarget = null; }}>
        Translation{#if translation.enabled}<i class="dot" aria-label="(on)"></i>{/if}
      </button>
      <span class="tab-thumb" class:right={tab === "translation"} aria-hidden="true"></span>
    </div>

    {#if tab === "general"}
    <div class="tab-panel" role="tabpanel" id="panel-general" aria-labelledby="tab-general">
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
      {@render recorder("trigger", "Shortcut", shortcut)}
      {@render recorder("replace", "Replace key", replaceShortcut)}
      <div class="row preference-row">
        <div class="preference-label">
          <span id="show-changes-label">Show changes</span>
          <p id="show-changes-description">Highlight edits after proofreading.</p>
        </div>
        <button class="switch" class:on={showChanges} type="button" role="switch" aria-checked={showChanges} aria-labelledby="show-changes-label" aria-describedby="show-changes-description" disabled={isTogglingChanges} onclick={toggleShowChanges}><i></i></button>
      </div>
      {#if changesNote}<p class="row note" role="alert">{changesNote}</p>{/if}
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
    </div>
    {:else}
    <div class="tab-panel" role="tabpanel" id="panel-translation" aria-labelledby="tab-translation">
      <div class="panel">
        <div class="row preference-row">
          <div class="preference-label">
            <span id="translator-label">Translator</span>
            <p id="translator-description">Translate any selected text with its own shortcut.</p>
          </div>
          <button class="switch" class:on={translation.enabled} type="button" role="switch" aria-checked={translation.enabled} aria-labelledby="translator-label" aria-describedby="translator-description" disabled={isSavingTranslation} onclick={() => saveTranslation({ enabled: !translation.enabled })}><i></i></button>
        </div>
      </div>

      <div class="panel" class:off={!translation.enabled} inert={!translation.enabled}>
        <div class="row">
          <span>Translate to</span>
          <button class="picker-button" type="button" aria-haspopup="listbox" aria-expanded={pickerOpen} disabled={isSavingTranslation} onclick={openPicker}>
            <span class="lang">
              {#if currentLanguage}<b dir={currentLanguage.rtl ? "rtl" : "ltr"}>{currentLanguage.native}</b>{#if currentLanguage.native !== currentLanguage.name}<small>{currentLanguage.name}</small>{/if}{:else}<b>{translation.language}</b>{/if}
            </span>
            <svg viewBox="0 0 12 12" aria-hidden="true" class:flipped={pickerOpen}><path d="m3 4.5 3 3 3-3" /></svg>
          </button>
        </div>
        {#if pickerOpen}
          <div class="picker">
            <input
              bind:this={languageSearch}
              bind:value={languageQuery}
              onkeydown={pickerKeydown}
              type="search"
              placeholder="Search languages"
              spellcheck="false"
              autocomplete="off"
              role="combobox"
              aria-expanded="true"
              aria-controls="language-list"
              aria-activedescendant={languageMatches[highlighted] ? `language-${languageMatches[highlighted].code}` : undefined}
            />
            <ul id="language-list" role="listbox" aria-label="Languages" bind:this={languageList}>
              {#each languageMatches as language, index (language.code)}
                <!-- svelte-ignore a11y_click_events_have_key_events (the search box drives the list from the keyboard) -->
                <li
                  id="language-{language.code}"
                  data-index={index}
                  role="option"
                  aria-selected={language.name === translation.language}
                  class:highlighted={index === highlighted}
                  onpointermove={() => (highlighted = index)}
                  onmousedown={(event) => event.preventDefault()}
                  onclick={() => chooseLanguage(language)}
                >
                  <span class="lang">
                    <b dir={language.rtl ? "rtl" : "ltr"}>{language.native}</b>
                    {#if language.native !== language.name}<small>{language.name}</small>{/if}
                  </span>
                  {#if language.rtl}<span class="dir-chip" title="Right-to-left">RTL</span>{/if}
                  {#if language.name === translation.language}
                    <svg class="check" viewBox="0 0 12 12" aria-hidden="true"><path d="m2.5 6.2 2.2 2.2 4.8-5" /></svg>
                  {/if}
                </li>
              {:else}
                <li class="empty" role="presentation">No language matches “{languageQuery}”</li>
              {/each}
            </ul>
          </div>
        {/if}
        <div class="row">
          <span>Direction</span>
          <span class="dir-value">
            {#if currentLanguage?.rtl}
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M13 5H4m0 0 2.5-2.5M4 5l2.5 2.5M13 11H6" /></svg>Right to left
            {:else}
              <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M3 5h9m0 0L9.5 2.5M12 5 9.5 7.5M3 11h7" /></svg>Left to right
            {/if}
          </span>
        </div>
        {@render recorder("translate", "Shortcut", translation.shortcut, isSavingTranslation)}
      </div>
      {#if translationNote}<p class="inline-note" role="alert">{translationNote}</p>{/if}

      <p class="hint">
        {#if translation.enabled}
          Select text in any app and press
          {#each shortcutParts(translation.shortcut) as part}<kbd>{part}</kbd>{/each}
          to translate it into {currentLanguage?.name ?? translation.language}.
        {:else}
          Turn on the translator to choose a language and shortcut.
        {/if}
      </p>

      <div class="footer">
        <button class="link quiet" type="button" onclick={previewTranslation} disabled={!translation.enabled}>Preview translation</button>
      </div>
    </div>
    {/if}
  </main>
{/if}

<style>
  .shell { display: flex; flex-direction: column; gap: 14px; height: 100vh; padding: 20px 20px 16px; user-select: none; overflow-y: auto; }

  .panel { flex: none; margin: 0; overflow: hidden; }
  .row { display: flex; align-items: center; justify-content: space-between; gap: 12px; min-height: 46px; padding: 0 14px; }
  .row + .row { border-top: 1px solid var(--separator); }
  .row > span:first-child { flex: none; min-width: 88px; }
  .row input { flex: 1; min-width: 0; height: 30px; padding: 0 10px; border: 1px solid var(--field-edge); border-radius: 8px; outline: 0; background: var(--field); user-select: text; text-overflow: ellipsis; transition: border-color 0.15s ease, box-shadow 0.15s ease; }
  .row input::placeholder { color: var(--secondary); opacity: 0.7; }
  .row input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
  .preference-row { min-height: 62px; }
  .preference-label { min-width: 0; padding-block: 10px; }
  .preference-label p { margin: 4px 0 0; color: var(--secondary); font-size: 11px; line-height: 1.4; }

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

  .tabs { position: relative; display: grid; grid-template-columns: 1fr 1fr; flex: none; padding: 2px; border-radius: 9px; background: var(--key); }
  .tabs button { position: relative; z-index: 1; display: inline-flex; align-items: center; justify-content: center; gap: 6px; height: 26px; padding: 0; border: 0; border-radius: 7px; background: transparent; color: var(--secondary); font-size: 12px; font-weight: 500; cursor: pointer; transition: color 0.15s ease; }
  .tabs button.active { color: var(--label); }
  .tabs button:focus-visible { outline: 2px solid var(--accent); outline-offset: -2px; }
  .tab-thumb { position: absolute; top: 2px; left: 2px; width: calc(50% - 2px); height: 26px; border-radius: 7px; background: var(--panel); box-shadow: 0 1px 2px rgba(0, 0, 0, 0.12), 0 0 0 0.5px rgba(0, 0, 0, 0.04); transition: transform 0.22s cubic-bezier(0.3, 0.7, 0.2, 1); }
  .tab-thumb.right { transform: translateX(100%); }
  .dot { width: 6px; height: 6px; border-radius: 50%; background: var(--success); }
  .tab-panel { display: flex; flex-direction: column; gap: 14px; flex: 1; animation: fade-in 0.18s ease; }
  @keyframes fade-in { from { opacity: 0; transform: translateY(3px); } }

  .panel.off { opacity: 0.5; transition: opacity 0.2s ease; }
  .panel { transition: opacity 0.2s ease; }
  .lang { display: inline-flex; align-items: baseline; gap: 6px; min-width: 0; }
  .lang b { font-weight: 500; unicode-bidi: isolate; }
  .lang small { color: var(--secondary); font-size: 11px; }
  .picker-button { display: inline-flex; align-items: center; gap: 6px; min-height: 26px; padding: 0 6px 0 8px; border: 0; border-radius: 8px; background: transparent; color: var(--label); cursor: pointer; }
  .picker-button:hover:not(:disabled) { background: var(--key); }
  .picker-button svg { width: 10px; height: 10px; fill: none; stroke: var(--secondary); stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.6; transition: transform 0.18s ease; }
  .picker-button svg.flipped { transform: rotate(180deg); }
  .picker { padding: 0 8px 8px; border-top: 1px solid var(--separator); animation: fade-in 0.15s ease; }
  .picker input { width: 100%; height: 30px; margin: 8px 0 6px; padding: 0 10px; border: 1px solid var(--field-edge); border-radius: 8px; outline: 0; background: var(--field); user-select: text; }
  .picker input:focus { border-color: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 22%, transparent); }
  .picker ul { max-height: 196px; margin: 0; padding: 0; overflow-y: auto; list-style: none; scrollbar-width: thin; }
  .picker li { display: flex; align-items: center; gap: 8px; min-height: 30px; padding: 0 8px; border-radius: 7px; cursor: pointer; }
  .picker li.highlighted { background: color-mix(in srgb, var(--accent) 14%, transparent); }
  .picker li[aria-selected="true"] b { color: var(--accent); }
  .picker li .lang { flex: 1; }
  .picker li.empty { justify-content: center; color: var(--secondary); font-size: 12px; cursor: default; }
  .dir-chip { padding: 1px 5px; border-radius: 4px; background: var(--key); color: var(--secondary); font-size: 9px; font-weight: 600; letter-spacing: 0.04em; }
  .check { width: 12px; height: 12px; fill: none; stroke: var(--accent); stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.8; }
  .dir-value { display: inline-flex; align-items: center; gap: 6px; color: var(--secondary); font-size: 12px; }
  .dir-value svg { width: 14px; height: 14px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.4; }
  .inline-note { margin: -6px 4px 0; color: var(--danger); font-size: 12px; line-height: 16px; user-select: text; }
  .hint { margin: 0 4px; color: var(--secondary); font-size: 12px; line-height: 1.7; }
  .hint kbd { margin: 0 1px; }
  .link:disabled { opacity: 0.4; cursor: default; }
  @media (prefers-reduced-motion: reduce) { .tab-thumb, .tab-panel, .picker { transition: none; animation: none; } }

  .debug-e2e-shell { display: grid; place-items: center; min-height: 100vh; padding: 30px; }
  .debug-e2e-shell textarea { width: 100%; min-height: 180px; }
</style>
