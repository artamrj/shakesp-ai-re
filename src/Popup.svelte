<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import { onMount, tick } from "svelte";
  import { eventShortcut, shortcutParts } from "./shortcut";

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
          isStreaming = false;
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
    requestedHeight = wanted;
    void invoke("resize_popup", { height: wanted }).catch(() => {});
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

    try {
      const [capturedText, testMode, contextError] = await Promise.all([
        invoke<string>("get_popup_selection"),
        invoke<boolean>("get_popup_test_mode"),
        invoke<string>("get_popup_error"),
      ]);
      if (generation !== requestGeneration) return;
      if (contextError) {
        popupError = contextError;
        return;
      }
      if (!capturedText.trim()) return;
      selectedText = capturedText;
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
    if (isTestPopup || isStreaming || isApplying || popupError || !outputText.trim()) return;
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
    if (event.key === "Escape") {
      event.preventDefault();
      void closePopup();
    } else if (!event.isComposing && eventShortcut(event) === replaceShortcut) {
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
      <strong>Proofread</strong>
    </div>
    {#if isStreaming}
      <span class="stream-status"><i></i>{streamStatus}</span>
    {:else if outputText && !popupError}
      <span class="done-status">
        <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2.5 6 2.1 2.1 4.9-4.8" /></svg>
        Ready
      </span>
    {/if}
    <button class="icon-button" aria-label="Close" title="Close" tabindex="-1" onclick={closePopup}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8" /></svg>
    </button>
  </header>

  <section bind:this={resultElement} class="result" class:loading={isStreaming} aria-live="polite" aria-busy={isStreaming}>
    {#if outputText}
      <div class="text" dir="auto">{outputText}{#if isStreaming}<span class="stream-cursor" aria-hidden="true"></span>{/if}</div>
    {:else if !popupError}
      <div class="skeleton" aria-label="Preparing your proofread result"><i></i><i></i><i></i></div>
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
</main>

<style>
  .popup-shell { position: relative; display: flex; flex-direction: column; height: 100vh; overflow: hidden; border: 1px solid var(--panel-edge); border-radius: 16px; box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.32); }
  :global(html[data-platform="linux"]) .popup-shell { background: var(--bg-solid); }

  .popup-header { display: flex; align-items: center; flex: none; height: 38px; padding: 0 10px 0 14px; border-bottom: 1px solid var(--separator); user-select: none; }
  .popup-title { color: var(--secondary); font-size: 12px; font-weight: 600; }
  .icon-button { display: grid; place-items: center; width: 22px; height: 22px; margin-left: 6px; padding: 0; border: 0; border-radius: 50%; background: transparent; color: var(--secondary); cursor: pointer; }
  .icon-button svg { width: 12px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-width: 1.5; }
  .icon-button:focus-visible { outline: none; }
  .icon-button:hover { background: var(--key); color: var(--label); }

  .stream-status, .done-status { display: flex; align-items: center; gap: 6px; margin-left: auto; font-size: 11px; }
  .stream-status { color: var(--secondary); }
  .stream-status i { width: 6px; height: 6px; border-radius: 50%; background: var(--accent); box-shadow: 0 0 0 3px color-mix(in srgb, var(--accent) 16%, transparent); animation: pulse 1s infinite alternate; }
  .done-status { gap: 4px; color: var(--success); font-weight: 500; }
  .done-status svg { width: 10px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.6; }

  .result { flex: 1 1 auto; min-height: 0; padding: 12px 14px; overflow-y: auto; scrollbar-color: color-mix(in srgb, var(--label) 20%, transparent) transparent; scrollbar-width: thin; }
  .text { color: var(--label); line-height: 1.5; overflow-wrap: anywhere; white-space: pre-wrap; unicode-bidi: plaintext; text-align: start; }
  .text :global(.stream-cursor) { display: inline-block; width: 2px; height: 1em; margin-inline-start: 2px; border-radius: 2px; background: var(--accent); vertical-align: -0.15em; animation: blink 0.72s steps(1) infinite; }

  .skeleton { display: grid; gap: 12px; padding-top: 3px; }
  .skeleton i { display: block; height: 10px; border-radius: 999px; background: linear-gradient(90deg, var(--key) 20%, color-mix(in srgb, var(--label) 16%, transparent) 40%, var(--key) 60%); background-size: 300% 100%; animation: shimmer 1.35s ease infinite; }
  .skeleton i:nth-child(2) { width: 92%; }
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
