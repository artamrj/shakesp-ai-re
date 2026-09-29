// Shortcut strings look like "Control+Shift+Space": modifiers, then a KeyboardEvent.code.

const supportedShortcutCodes = new Set([
  "Backquote", "Backslash", "BracketLeft", "BracketRight", "Comma", "Equal",
  "Minus", "Period", "Quote", "Semicolon", "Slash", "Space", "Tab", "Enter",
  "Backspace", "Delete", "End", "Home", "Insert", "PageDown", "PageUp",
  "ArrowDown", "ArrowLeft", "ArrowRight", "ArrowUp",
]);

export function isSupportedShortcutCode(code: string) {
  return supportedShortcutCodes.has(code) || /^(Key[A-Z]|Digit[0-9]|F(?:[1-9]|1[0-9]|2[0-4]))$/.test(code);
}

export function shortcutParts(value: string, isMac: boolean) {
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

export function eventModifiers(event: KeyboardEvent) {
  return [
    event.ctrlKey && "Control",
    event.altKey && "Alt",
    event.shiftKey && "Shift",
    event.metaKey && "Super",
  ].filter((part): part is string => Boolean(part));
}

/** The shortcut string for a key press, in the same form the recorder saves. */
export function eventShortcut(event: KeyboardEvent): string {
  return [...eventModifiers(event), event.code].join("+");
}
