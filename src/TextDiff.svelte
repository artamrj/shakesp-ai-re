<script lang="ts">
  import { visibleWhitespace, type DiffPart } from "./text-diff";

  let { parts }: { parts: DiffPart[] } = $props();

  const isWhitespace = (value: string) => /^\s+$/.test(value);
  const displayValue = (value: string) => isWhitespace(value) ? visibleWhitespace(value) : value;
</script>

<!-- Keep fragments adjacent so template whitespace cannot alter the preview. -->
<div class="text-diff" dir="auto">{#each parts as part}{#if part.kind === "removed"}<del title="Removed" aria-label={isWhitespace(part.value) ? "Removed whitespace" : undefined}>{displayValue(part.value)}</del>{:else if part.kind === "added"}<ins title="Added" aria-label={isWhitespace(part.value) ? "Added whitespace" : undefined}>{displayValue(part.value)}</ins>{:else}{part.value}{/if}{/each}</div>

<style>
  .text-diff { color: var(--label); line-height: 1.5; overflow-wrap: anywhere; white-space: pre-wrap; unicode-bidi: plaintext; text-align: start; }
  del { color: var(--diff-removed); background: var(--diff-removed-bg); text-decoration-thickness: 1px; }
  ins { color: var(--diff-added); background: var(--diff-added-bg); text-decoration-line: underline; text-decoration-thickness: 1px; text-underline-offset: 3px; }
  del, ins { border-radius: 3px; box-decoration-break: clone; -webkit-box-decoration-break: clone; }
</style>
