import { diffArrays } from "diff";

export interface DiffPart {
  kind: "equal" | "added" | "removed";
  value: string;
}

export interface TextDiff {
  parts: DiffPart[];
  /** Consecutive additions/deletions count as one edit, not two corrections. */
  changeCount: number;
}

const MAX_TEXT_LENGTH = 100_000;
const MAX_TOKENS = 20_000;
const segmenter = typeof Intl.Segmenter === "function"
  ? new Intl.Segmenter(undefined, { granularity: "word" })
  : null;

function tokenize(text: string): string[] {
  if (segmenter) {
    return Array.from(segmenter.segment(text), ({ segment }) => segment);
  }
  // Older webviews still get exact whitespace and Unicode-safe tokens.
  return text.match(/[\p{L}\p{M}\p{N}_]+|[^\p{L}\p{M}\p{N}_]/gu) ?? [];
}

/** A display-only diff. Neither input is normalized or used as HTML. */
export function compareText(original: string, corrected: string): TextDiff | null {
  if (original === corrected) {
    return { parts: [{ kind: "equal", value: corrected }], changeCount: 0 };
  }
  // Keep a large or heavily rewritten selection from blocking the popup.
  if (original.length + corrected.length > MAX_TEXT_LENGTH) return null;
  const before = tokenize(original);
  const after = tokenize(corrected);
  if (before.length + after.length > MAX_TOKENS) return null;
  const changes = diffArrays(before, after, { timeout: 50, maxEditLength: 1_500 });
  if (!changes) return null;

  let changeCount = 0;
  let inChange = false;
  const parts: DiffPart[] = changes.map((change) => {
    const kind = change.added ? "added" : change.removed ? "removed" : "equal";
    if (kind === "equal") {
      inChange = false;
    } else if (!inChange) {
      changeCount += 1;
      inChange = true;
    }
    return { kind, value: change.value.join("") };
  });
  return { parts, changeCount };
}

/** Make whitespace-only edits visible without changing the corrected output. */
export function visibleWhitespace(value: string): string {
  return value.replace(/\r\n|\s/gu, (character) => {
    if (character === "\t") return "⇥";
    if (/^[\r\n\u2028\u2029]+$/.test(character)) return "↵\n";
    return "·";
  });
}

/** Models drop the selection's surrounding whitespace, e.g. a trailing newline. Restore it. */
export function keepOuterWhitespace(original: string, corrected: string): string {
  const leading = original.match(/^\s*/u)![0];
  const trailing = original.slice(leading.length).match(/\s*$/u)![0];
  const core = corrected.trim();
  return core ? leading + core + trailing : corrected;
}
