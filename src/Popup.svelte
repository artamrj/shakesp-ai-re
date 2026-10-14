<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { onMount, tick } from "svelte";
  import { eventShortcut, shortcutParts } from "./shortcut";
  import TextDiff from "./TextDiff.svelte";
  import { compareText, keepOuterWhitespace } from "./text-diff";
  import { findLanguage } from "./languages";

  let selectedText = $state("");
  let outputText = $state("");
  let popupError = $state("");
  let isStreaming = $state(false);
  let streamStatus = $state("Connecting");
  let retryMessage = $state("");
  let isApplying = $state(false);
  let isTestPopup = $state(false);
  let copyState = $state<"idle" | "copied" | "error">("idle");
  let startTimer: number | undefined;
  let renderFrame: number | undefined;
  let copyTimer: number | undefined;
  let pendingOutput = "";
  let requestGeneration = 0;
  let requestedHeight = 0;
  let replaceShortcut = $state("Enter");
  let showChanges = $state(false);
  let resultView = $state<"result" | "changes">("changes");
  let isComplete = $state(false);
  let isTranslate = $state(false);
  let targetLanguage = $state("English");
  const target = $derived(findLanguage(targetLanguage));
  const unchanged = $derived(isComplete && !popupError && outputText === selectedText);
  // Compare once the final result arrives; partial streams are not meaningful edits.
  const comparison = $derived(showChanges && !isTranslate && isComplete && !popupError
    ? compareText(selectedText, outputText)
    : null);
  const reviewingChanges = $derived(resultView === "changes" && comparison !== null && comparison.changeCount > 0);
  const isMac = navigator.userAgent.includes("Mac");
  let resultElement: HTMLElement;

  interface StreamRetryEvent {
    attempt: number;
    maxAttempts: number;
    message: string;
  }

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];
    let disposed = false;

    async function setup() {
      unlisteners.push(
        await listen<string>("ai-stream-chunk", (event) => {
          streamStatus = "Writing";
          retryMessage = "";
          queueOutput(event.payload);
        }),
        await listen<StreamRetryEvent>("ai-stream-retry", (event) => {
          streamStatus = `Retrying ${event.payload.attempt}/${event.payload.maxAttempts}`;
          retryMessage = event.payload.message;
        }),
        await listen<string>("ai-stream-error", (event) => {
          flushOutput();
          popupError = event.payload;
          isStreaming = false;
          retryMessage = "";
          void tick().then(() => {
            growIfScrollable();
            if (resultElement) resultElement.scrollTop = resultElement.scrollHeight;
          });
        }),
        await listen("ai-stream-done", () => {
          flushOutput();
          outputText = keepOuterWhitespace(selectedText, outputText);
          isStreaming = false;
          isComplete = true;
          retryMessage = "";
          void invoke("debug_e2e_report", {
            selectedText,
            outputText,
            error: popupError,
          }).catch(() => {});
        }),
        await listen("popup-reset", () => {
          window.focus();
          scheduleProofread();
        }),
        await listen<boolean>("show-changes-changed", (event) => {
          showChanges = event.payload;
          resultView = "changes";
        }),
      );

      window.addEventListener("keydown", handleKeydown);
      scheduleProofread();
    }

    setup().catch((error) => {
      if (!disposed) popupError = String(error);
    });

    return () => {
      disposed = true;
      requestGeneration += 1;
      if (startTimer !== undefined) window.clearTimeout(startTimer);
      if (renderFrame !== undefined) window.cancelAnimationFrame(renderFrame);
      if (copyTimer !== undefined) window.clearTimeout(copyTimer);
      window.removeEventListener("keydown", handleKeydown);
      for (const unlisten of unlisteners) unlisten();
    };
  });

  $effect(() => {
    // Fit the finished view, including shrinking when highlights are hidden.
    reviewingChanges;
    const completed = isComplete && !popupError;
    void tick().then(() => {
      if (!completed || !resultElement) return;
      fitCompletedResult();
      resultElement.scrollTop = 0;
    });
  });

  function scheduleProofread() {
    if (startTimer !== undefined) window.clearTimeout(startTimer);
    startTimer = window.setTimeout(() => {
      startTimer = undefined;
      void startProofread();
    }, 0);
  }

  function queueOutput(chunk: string) {
    pendingOutput += chunk;
    if (renderFrame !== undefined) return;
    renderFrame = window.requestAnimationFrame(() => {
      renderFrame = undefined;
      flushOutput();
    });
  }

  function flushOutput() {
    if (!pendingOutput) return;
    outputText += pendingOutput;
    pendingOutput = "";
    void tick().then(() => {
      growIfScrollable();
      if (resultElement) resultElement.scrollTop = resultElement.scrollHeight;
    });
  }

  // Grow the window a little (the backend caps it) when the result overflows.
  function growIfScrollable() {
    if (!resultElement) return;
    const overflow = resultElement.scrollHeight - resultElement.clientHeight;
    if (overflow <= 1) return;
    const wanted = Math.ceil(window.innerHeight + overflow);
    if (wanted <= requestedHeight) return;
    requestPopupHeight(wanted);
  }

  function fitCompletedResult() {
    const content = resultElement?.firstElementChild;
    if (!(content instanceof HTMLElement)) return;
    const style = window.getComputedStyle(resultElement);
    const padding = parseFloat(style.paddingTop) + parseFloat(style.paddingBottom);
    const chrome = window.innerHeight - resultElement.clientHeight;
    // The backend clamps this to the popup's existing minimum and maximum sizes.
    requestPopupHeight(Math.ceil(chrome + content.getBoundingClientRect().height + padding));
  }

  function requestPopupHeight(height: number) {
    if (height === requestedHeight) return;
    requestedHeight = height;
    void invoke("resize_popup", { height }).catch(() => {});
  }

  async function startProofread() {
    const generation = ++requestGeneration;
    // Pick up a changed replace key without restarting the app.
    void invoke<string>("get_replace_shortcut").then((value) => (replaceShortcut = value)).catch(() => {});
    requestedHeight = 0;
    selectedText = "";
    outputText = "";
    pendingOutput = "";
    if (renderFrame !== undefined) {
      window.cancelAnimationFrame(renderFrame);
      renderFrame = undefined;
    }
    popupError = "";
    isStreaming = false;
    streamStatus = "Connecting";
    retryMessage = "";
    isApplying = false;
    isTestPopup = false;
    copyState = "idle";
    isComplete = false;
    resultView = "changes";

    try {
      const [capturedText, testMode, contextError, changesEnabled, translateMode, translation] = await Promise.all([
        invoke<string>("get_popup_selection"),
        invoke<boolean>("get_popup_test_mode"),
        invoke<string>("get_popup_error"),
        invoke<boolean>("get_show_changes").catch(() => false),
        invoke<boolean>("get_popup_translate").catch(() => false),
        invoke<{ language: string }>("get_translation_settings").catch(() => null),
      ]);
      if (generation !== requestGeneration) return;
      isTranslate = translateMode;
      if (translation) targetLanguage = translation.language;
      if (contextError) {
        popupError = contextError;
        return;
      }
      if (!capturedText.trim()) return;
      selectedText = capturedText;
      showChanges = changesEnabled;
      isTestPopup = testMode;
      isStreaming = true;
      await invoke("stream_ai_text", { selectedText });
    } catch (error) {
      if (generation === requestGeneration) {
        popupError = String(error);
        isStreaming = false;
      }
    }
  }

  async function applyReplacement() {
    if (isTranslate || isTestPopup || isStreaming || isApplying || popupError || !outputText.trim()) return;
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
    flushOutput();
    if (!outputText.trim()) return;
    try {
      await writeText(outputText);
      copyState = "copied";
      if (copyTimer !== undefined) window.clearTimeout(copyTimer);
      copyTimer = window.setTimeout(() => (copyState = "idle"), 1600);
    } catch {
      copyState = "error";
      if (copyTimer !== undefined) window.clearTimeout(copyTimer);
      copyTimer = window.setTimeout(() => (copyState = "idle"), 2000);
    }
  }

  async function closePopup() {
    requestGeneration += 1;
    isStreaming = false;
    retryMessage = "";
    if (startTimer !== undefined) {
      window.clearTimeout(startTimer);
      startTimer = undefined;
    }
    await invoke("close_popup");
  }

  function handleKeydown(event: KeyboardEvent) {
    // Enter/Space should activate a focused control, including the highlights button,
    // rather than unexpectedly replacing the selection behind the popup.
    const activatesButton = event.target instanceof HTMLElement
      && !!event.target.closest("button")
      && (event.key === "Enter" || event.key === " ");
    if (event.key === "Escape") {
      event.preventDefault();
      void closePopup();
    } else if (isTranslate) {
      // Translations have no Replace or Copy: selecting the text and copying works natively.
      return;
    } else if (!event.isComposing && !activatesButton && eventShortcut(event) === replaceShortcut) {
      event.preventDefault();
      void applyReplacement();
    } else if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "c") {
      event.preventDefault();
      void copyResult();
    }
  }
</script>

<main class="popup-shell">
  <header class="popup-header">
    <div class="popup-title">
      {#if isTranslate}
        <strong class="translate-title">
          Translate
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2 6h8m0 0L7 3m3 3L7 9" /></svg>
          <span dir={target?.rtl ? "rtl" : "ltr"}>{target?.native ?? targetLanguage}</span>
        </strong>
      {:else}
        <strong>Proofread</strong>
      {/if}
    </div>
    <div class="header-actions">
      {#if isStreaming}
        <span class="stream-status"><i></i>{streamStatus}</span>
      {:else if unchanged}
        <span class="done-status">
          <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2.5 6 2.1 2.1 4.9-4.8" /></svg>
          {isTranslate ? `Already in ${target?.name ?? targetLanguage}` : "Already looks good"}
        </span>
      {/if}
      {#if comparison && comparison.changeCount > 0}
        <button
          class="icon-button change-toggle"
          class:active={reviewingChanges}
          type="button"
          aria-label="Show changes"
          aria-pressed={reviewingChanges}
          title={reviewingChanges ? "Show clean result" : "Show changes"}
          onclick={() => (resultView = reviewingChanges ? "result" : "changes")}
        >
          <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M1.5 8s2.5-4.5 6.5-4.5S14.5 8 14.5 8s-2.5 4.5-6.5 4.5S1.5 8 1.5 8Z" /><circle cx="8" cy="8" r="2" /></svg>
        </button>
      {/if}
      <button class="icon-button" aria-label="Close" title="Close" tabindex="-1" onclick={closePopup}>
        <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8" /></svg>
      </button>
    </div>
  </header>

  <section bind:this={resultElement} class="result" class:loading={isStreaming} aria-live="polite" aria-busy={isStreaming}>
    {#if outputText}
      {#if reviewingChanges && comparison}
        <TextDiff parts={comparison.parts} />
      {:else}
        <div class="text" class:translation={isTranslate} dir={isTranslate ? (target?.rtl ? "rtl" : "ltr") : "auto"} lang={isTranslate ? target?.code : undefined}>{outputText}{#if isStreaming}<span class="stream-cursor" aria-hidden="true"></span>{/if}</div>
      {/if}
    {:else if !popupError}
      <div class="skeleton" dir={isTranslate && target?.rtl ? "rtl" : "ltr"} aria-label={isTranslate ? "Preparing your translation" : "Preparing your proofread result"}><i></i><i></i><i></i></div>
    {/if}
    {#if isStreaming && retryMessage && !outputText}
      <p class="retry-note">{retryMessage}</p>
    {/if}
    {#if popupError}
      <div class="error" role="alert">
        <svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="8" /><path d="M10 6v5m0 3h.01" /></svg>
        <div>
          <p>{popupError}</p>
          <button class="btn btn-sm btn-secondary" onclick={startProofread}>Try again</button>
        </div>
      </div>
    {/if}
  </section>

  {#if !isTranslate}
  <footer class="popup-actions">
    <button class="btn btn-sm btn-primary" onclick={applyReplacement} disabled={isTestPopup || isStreaming || isApplying || !!popupError || !outputText.trim()} title={isTestPopup ? "Preview only. Nothing is replaced." : `Replace selected text (${shortcutParts(replaceShortcut, isMac).join(" ")})`}>
      {#if isApplying}<span class="button-spinner"></span>{/if}
      {isApplying ? "Replacing" : "Replace"}
    </button>
    <button class="btn btn-sm btn-secondary" onclick={copyResult} disabled={!outputText.trim()} title="Copy result">
      {#if copyState === "copied"}
        <svg viewBox="0 0 14 14" aria-hidden="true"><path d="m2.5 7 2.7 2.7 6-6" /></svg>
      {:else}
        <svg viewBox="0 0 14 14" aria-hidden="true"><rect x="4.5" y="4.5" width="7" height="7" rx="1.5" /><path d="M9.5 4.5V3.2c0-.9-.7-1.7-1.7-1.7H3.2c-.9 0-1.7.7-1.7 1.7v4.6c0 .9.7 1.7 1.7 1.7h1.3" /></svg>
      {/if}
      {copyState === "copied" ? "Copied" : copyState === "error" ? "Copy failed" : "Copy"}
    </button>
  </footer>
  {/if}
</main>

<style>
  .popup-shell { position: relative; display: flex; flex-direction: column; height: 100vh; overflow: hidden; border: 1px solid var(--panel-edge); border-radius: 16px; box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.32); }
  :global(html[data-platform="linux"]) .popup-shell { background: var(--bg-solid); }

  .popup-header { display: flex; align-items: center; flex: none; height: 38px; padding: 0 10px 0 14px; border-bottom: 1px solid var(--separator); user-select: none; }
  .popup-title { min-width: 0; color: var(--secondary); font-size: 12px; font-weight: 600; }
  .translate-title { display: inline-flex; align-items: center; gap: 5px; }
  .translate-title svg { width: 10px; height: 10px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.5; }
  .translate-title span { color: var(--accent); unicode-bidi: isolate; }
  .header-actions { display: flex; align-items: center; gap: 4px; margin-left: auto; }
  .icon-button { display: grid; place-items: center; flex: none; width: 22px; height: 22px; padding: 0; border: 0; border-radius: 50%; background: transparent; color: var(--secondary); cursor: pointer; }
  .icon-button svg { width: 12px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-width: 1.5; }
  .icon-button:hover { background: var(--key); color: var(--label); }
  .change-toggle.active { background: var(--key); color: var(--accent); }

  .stream-status, .done-status { display: flex; align-items: center; gap: 6px; font-size: 11px; }
  .stream-status { color: var(--secondary); }
  .stream-status i { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 16%, transparent); animation: pulse 1s infinite alternate; }
  .done-status { gap: 4px; color: var(--success); font-weight: 500; }
  .done-status svg { width: 10px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.6; }

  .result { flex: 1 1 auto; min-height: 0; padding: 12px 14px; overflow-y: auto; scrollbar-color: color-mix(in srgb, var(--label) 20%, transparent) transparent; scrollbar-width: thin; }
  .text { color: var(--label); line-height: 1.5; overflow-wrap: anywhere; white-space: pre-wrap; unicode-bidi: plaintext; text-align: start; }
  /* The target language sets the direction; isolate keeps embedded names and numbers in order. */
  .text.translation { unicode-bidi: isolate; user-select: text; cursor: text; }
  .text :global(.stream-cursor) { display: inline-block; width: 2px; height: 1em; margin-inline-start: 2px; border-radius: 2px; background: var(--accent); vertical-align: -0.15em; animation: blink 0.72s steps(1) infinite; }

  .skeleton { display: grid; gap: 12px; padding-top: 3px; }
  .skeleton i { display: block; height: 10px; border-radius: 999px; background: linear-gradient(90deg, var(--key) 20%, color-mix(in srgb, var(--label) 16%, transparent) 40%, var(--key) 60%); background-size: 300% 100%; animation: shimmer 1.35s ease infinite; }
  .skeleton i:nth-child(2) { width: 92%; }
  .skeleton[dir="rtl"] i { justify-self: end; }
  .skeleton i:nth-child(3) { width: 54%; }
  .retry-note { margin: 9px 0 0; color: var(--secondary); font-size: 11px; line-height: 1.35; }

  .error { display: flex; align-items: flex-start; gap: 10px; margin-top: 8px; padding: 12px; border-radius: 12px; background: color-mix(in srgb, var(--danger) 10%, transparent); color: var(--danger); }
  .error svg { flex: none; width: 18px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-width: 1.6; }
  .error > div { min-width: 0; }
  .error p { margin: 0 0 8px; line-height: 1.45; }

  .popup-actions { display: flex; align-items: center; gap: 8px; flex: none; padding: 8px 12px 10px; border-top: 1px solid var(--separator); }
  .popup-actions .btn > svg { width: 12px; height: 12px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.4; }
  .button-spinner { display: inline-block; width: 10px; height: 10px; border: 1.5px solid rgba(255, 255, 255, 0.4); border-top-color: #fff; border-radius: 50%; animation: spin 0.7s linear infinite; }
  @keyframes pulse { to { opacity: 0.35; transform: scale(0.8); } }
  @keyframes blink { 50% { opacity: 0; } }
  @keyframes shimmer { to { background-position: -150% 0; } }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .stream-status i, .text :global(.stream-cursor), .skeleton i, .button-spinner { animation: none; } }
</style>
