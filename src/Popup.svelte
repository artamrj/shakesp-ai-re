<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { writeText } from "@tauri-apps/plugin-clipboard-manager";
  import DOMPurify from "dompurify";
  import { marked } from "marked";
  import { onMount, tick } from "svelte";

  let selectedText = $state("");
  let outputText = $state("");
  let popupError = $state("");
  let isStreaming = $state(false);
  let isApplying = $state(false);
  let copyState = $state<"idle" | "copied" | "error">("idle");
  let startTimer: number | undefined;
  let renderFrame: number | undefined;
  let copyTimer: number | undefined;
  let pendingOutput = "";
  let requestGeneration = 0;
  let resultElement: HTMLElement;
  let renderedMarkdown = $derived(renderMarkdown(outputText, isStreaming));

  onMount(() => {
    const unlisteners: UnlistenFn[] = [];
    let disposed = false;

    async function setup() {
      unlisteners.push(
        await listen<string>("ai-stream-chunk", (event) => {
          queueOutput(event.payload);
        }),
        await listen<string>("ai-stream-error", (event) => {
          flushOutput();
          popupError = event.payload;
          isStreaming = false;
        }),
        await listen("ai-stream-done", () => {
          flushOutput();
          isStreaming = false;
          void invoke("debug_e2e_report", {
            selectedText,
            outputText,
            error: popupError,
          }).catch(() => {});
        }),
        await listen("popup-reset", scheduleProofread),
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

  function renderMarkdown(source: string, streaming: boolean) {
    if (!source) return "";
    const parsed = marked.parse(source, { async: false, breaks: true, gfm: true }) as string;
    const safe = DOMPurify.sanitize(parsed, {
      ALLOWED_TAGS: ["p", "br", "strong", "b", "em", "i", "s", "code", "pre", "blockquote", "ul", "ol", "li", "h1", "h2", "h3", "h4", "h5", "h6", "hr", "table", "thead", "tbody", "tr", "th", "td"],
      ALLOWED_ATTR: [],
    });
    if (!streaming) return safe;

    const cursor = '<span class="stream-cursor" aria-hidden="true"></span>';
    const finalTextContainer = /<\/(p|li|h[1-6]|blockquote|pre)>\s*$/i;
    return finalTextContainer.test(safe)
      ? safe.replace(finalTextContainer, `${cursor}</$1>`)
      : `${safe}${cursor}`;
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
      if (resultElement) resultElement.scrollTop = resultElement.scrollHeight;
    });
  }

  async function startProofread() {
    const generation = ++requestGeneration;
    selectedText = "";
    outputText = "";
    pendingOutput = "";
    if (renderFrame !== undefined) {
      window.cancelAnimationFrame(renderFrame);
      renderFrame = undefined;
    }
    popupError = "";
    isStreaming = false;
    isApplying = false;
    copyState = "idle";

    try {
      const capturedText = await invoke<string>("get_popup_selection");
      if (generation !== requestGeneration || !capturedText.trim()) return;
      selectedText = capturedText;
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
    await invoke("close_popup");
  }

  function handleKeydown(event: KeyboardEvent) {
    if (event.key === "Escape") {
      event.preventDefault();
      void closePopup();
    } else if (event.key === "Enter" && !event.shiftKey && !event.metaKey && !event.ctrlKey) {
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
      <span class="stream-status"><i></i>Writing</span>
    {:else if outputText && !popupError}
      <span class="done-status">
        <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2.5 6 2.1 2.1 4.9-4.8" /></svg>
        Ready
      </span>
    {/if}
    <button class="icon-button" aria-label="Close" title="Close" onclick={closePopup}>
      <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m4 4 8 8m0-8-8 8" /></svg>
    </button>
  </header>

  <section bind:this={resultElement} class="result" class:loading={isStreaming} aria-live="polite" aria-busy={isStreaming}>
    {#if outputText}
      <div class="markdown">{@html renderedMarkdown}</div>
    {:else if !popupError}
      <div class="skeleton" aria-label="Preparing your proofread result"><i></i><i></i><i></i></div>
    {/if}
    {#if popupError}
      <div class="error" role="alert">
        <svg viewBox="0 0 20 20" aria-hidden="true"><circle cx="10" cy="10" r="8" /><path d="M10 6v5m0 3h.01" /></svg>
        <p>{popupError}</p>
      </div>
    {/if}
  </section>

  <footer class="popup-actions">
    <button class="primary" onclick={applyReplacement} disabled={isStreaming || isApplying || !outputText.trim()} title="Replace selected text">
      {#if isApplying}<span class="button-spinner"></span>{/if}
      {isApplying ? "Replacing" : "Replace"}
    </button>
    <button class="secondary" onclick={copyResult} disabled={!outputText.trim()} title="Copy result">
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
  :global(:root), :global(html), :global(body), :global(#app) { background: transparent; }
  :global(*) { box-sizing: border-box; }
  :global(button) { font: inherit; }
  .popup-shell { position: relative; display: flex; flex-direction: column; height: 100vh; overflow: hidden; border: 1px solid rgba(255,255,255,.22); border-radius: 16px; background: rgba(255,255,255,.025); box-shadow: inset 0 1px 0 rgba(255,255,255,.28); font-family: -apple-system, BlinkMacSystemFont, "Helvetica Neue", Arial, sans-serif; font-synthesis: none; font-weight: 300; -webkit-font-smoothing: antialiased; -moz-osx-font-smoothing: grayscale; }
  .popup-header { position: relative; z-index: 1; display: flex; align-items: center; flex: 0 0 auto; height: 36px; padding: 0 9px 0 14px; border-bottom: 1px solid rgba(87,94,108,.09); user-select: none; }
  .popup-title { display: flex; align-items: center; color: rgba(28,29,33,.80); font-size: 12px; font-weight: 400; }
  .popup-title strong { font-weight: 400; }
  .icon-button { display: grid; place-items: center; width: 22px; height: 22px; margin-left: 6px; padding: 0; border: 0; border-radius: 50%; background: transparent; color: rgba(55,59,66,.48); cursor: pointer; }
  .icon-button svg { width: 12px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-width: 1.4; }
  .icon-button:hover { background: rgba(60,64,72,.08); color: rgba(35,38,44,.72); }
  .result { position: relative; z-index: 1; flex: 1 1 auto; min-height: 0; padding: 11px 15px 10px; overflow-y: auto; scrollbar-color: rgba(95,103,120,.16) transparent; scrollbar-width: thin; }
  .markdown { color: rgba(38,40,46,.78); font-size: 12px; font-variation-settings: "wght" 300; font-weight: 300; letter-spacing: .005em; line-height: 1.52; overflow-wrap: anywhere; }
  .markdown :global(:first-child) { margin-top: 0; }
  .markdown :global(:last-child) { margin-bottom: 0; }
  .markdown :global(p) { margin: 0 0 8px; }
  .markdown :global(h1), .markdown :global(h2), .markdown :global(h3), .markdown :global(h4), .markdown :global(h5), .markdown :global(h6) { margin: 11px 0 5px; color: rgba(25,27,32,.86); font-size: 12px; font-weight: 500; line-height: 1.35; }
  .markdown :global(h1) { font-size: 14px; }
  .markdown :global(h2) { font-size: 13px; }
  .markdown :global(strong), .markdown :global(b) { font-weight: 500; color: rgba(27,29,34,.88); }
  .markdown :global(ul), .markdown :global(ol) { margin: 5px 0 8px; padding-left: 18px; }
  .markdown :global(li) { margin: 2px 0; padding-left: 1px; }
  .markdown :global(blockquote) { margin: 7px 0; padding: 2px 0 2px 9px; border-left: 2px solid rgba(104,87,217,.32); color: rgba(48,51,58,.65); }
  .markdown :global(code) { padding: 1px 4px; border-radius: 4px; background: rgba(72,77,88,.09); font-family: ui-monospace, SFMono-Regular, Menlo, monospace; font-size: 10.5px; }
  .markdown :global(pre) { margin: 7px 0; padding: 8px 9px; overflow-x: auto; border: 1px solid rgba(70,76,88,.08); border-radius: 8px; background: rgba(58,62,72,.07); line-height: 1.45; }
  .markdown :global(pre code) { padding: 0; background: transparent; }
  .markdown :global(hr) { margin: 9px 0; border: 0; border-top: 1px solid rgba(78,84,96,.11); }
  .markdown :global(table) { width: 100%; margin: 7px 0; border-collapse: collapse; font-size: 10.5px; }
  .markdown :global(th), .markdown :global(td) { padding: 4px 5px; border-bottom: 1px solid rgba(78,84,96,.10); text-align: left; }
  .markdown :global(th) { font-weight: 500; }
  .stream-status { display: flex; align-items: center; gap: 6px; margin-left: auto; color: #7d828c; font-size: 9px; font-weight: 300; }
  .stream-status i { width: 6px; height: 6px; border-radius: 50%; background: #7060df; box-shadow: 0 0 0 3px rgba(112,96,223,.10); animation: pulse 1s infinite alternate; }
  .done-status { display: flex; align-items: center; gap: 3px; margin-left: auto; color: rgba(59,112,76,.72); font-size: 9px; font-weight: 400; }
  .done-status svg { width: 10px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.5; }
  .markdown :global(.stream-cursor) { display: inline-block; width: 1.5px; height: .95em; margin-left: 2px; border-radius: 2px; background: #6857d9; vertical-align: -.1em; animation: blink .72s steps(1) infinite; }
  .skeleton { display: grid; gap: 12px; padding-top: 3px; }
  .skeleton i { display: block; height: 10px; border-radius: 999px; background: linear-gradient(90deg, rgba(119,127,143,.13) 20%, rgba(119,127,143,.23) 40%, rgba(119,127,143,.13) 60%); background-size: 300% 100%; animation: shimmer 1.35s ease infinite; }
  .skeleton i:nth-child(2) { width: 92%; }
  .skeleton i:nth-child(3) { width: 54%; }
  .error { display: flex; align-items: flex-start; gap: 10px; padding: 12px 13px; border: 1px solid rgba(189,74,64,.13); border-radius: 11px; background: rgba(244,92,81,.08); color: #a43d35; }
  .error svg { flex: 0 0 auto; width: 18px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-width: 1.6; }
  .error p { margin: 0; font-size: 12px; font-weight: 300; line-height: 1.45; }
  .popup-actions { position: relative; z-index: 1; display: flex; align-items: center; gap: 6px; flex: 0 0 auto; min-height: 40px; padding: 6px 10px 8px; border-top: 1px solid rgba(87,94,108,.09); }
  .popup-actions button { display: inline-flex; align-items: center; justify-content: center; gap: 4px; min-width: 52px; padding: 5px 9px; border: 0; border-radius: 8px; font-size: 9px; font-weight: 400; box-shadow: inset 0 1px 0 rgba(255,255,255,.22); cursor: pointer; transition: transform .15s ease, background .15s ease; }
  .popup-actions button > svg { width: 11px; height: 11px; fill: none; stroke: currentColor; stroke-linecap: round; stroke-linejoin: round; stroke-width: 1.25; }
  .popup-actions button:disabled { opacity: .48; cursor: default; }
  .popup-actions button:active:not(:disabled) { transform: translateY(1px); }
  .popup-actions .primary { background: rgba(42,44,51,.84); color: rgba(255,255,255,.94); }
  .popup-actions .primary:hover:not(:disabled) { background: rgba(36,39,46,.92); }
  .popup-actions .secondary { background: rgba(90,96,108,.10); color: rgba(35,38,44,.74); }
  .popup-actions .secondary:hover:not(:disabled) { background: rgba(90,96,108,.14); }
  .button-spinner { display: inline-block; width: 10px; height: 10px; margin-right: 5px; border: 1.5px solid rgba(255,255,255,.4); border-top-color: white; border-radius: 50%; animation: spin .7s linear infinite; }
  @keyframes pulse { to { opacity: .35; transform: scale(.8); } }
  @keyframes blink { 50% { opacity: 0; } }
  @keyframes shimmer { to { background-position: -150% 0; } }
  @keyframes spin { to { transform: rotate(360deg); } }
  @media (prefers-reduced-motion: reduce) { .stream-status i, .markdown :global(.stream-cursor), .skeleton i, .button-spinner { animation: none; } }
</style>
