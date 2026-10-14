// A small Markdown renderer for showing results, not for editing them.
// All text is HTML-escaped before any tag is added, and links are shown but never
// followed, so model output can never inject markup or navigate the popup.
// It tolerates half-finished input, so a streaming result renders as it arrives.

const blockStart = /^(?: {0,3}(?:#{1,6}\s|>|```|~~~|[-*+]\s|\d{1,9}[.)]\s|(?:[-*_]\s*){3,}$)|\s*\|.*\|\s*$)/;
const inlineMark = /(\*\*|__)\S.*?\1|`[^`\n]+`|\[[^\]\n]+\]\([^)\s]+\)|~~\S.*?~~/;

/** True when the text uses Markdown structure worth rendering. */
export function looksLikeMarkdown(text: string): boolean {
  const lines = text.split("\n");
  let structural = 0;
  for (const line of lines) {
    if (/^ {0,3}(#{1,6}\s|```|~~~|>\s?)/.test(line)) return true;
    if (/^\s*([-*+]|\d{1,9}[.)])\s+\S/.test(line)) structural += 1;
    if (/^\s*\|.*\|\s*$/.test(line)) structural += 1;
  }
  // One stray "- " or "1." in a sentence is not enough; a list needs two items.
  return structural >= 2 || inlineMark.test(text);
}

function escapeHtml(value: string): string {
  return value.replace(/&/g, "&amp;").replace(/</g, "&lt;").replace(/>/g, "&gt;").replace(/"/g, "&quot;");
}

/** Inline formatting on one already-unescaped line of text. */
export function renderInline(text: string): string {
  const codes: string[] = [];
  // Protect code spans first so their contents are never formatted.
  let html = escapeHtml(text).replace(/`([^`\n]+)`/g, (_, code: string) => {
    codes.push(`<code>${code}</code>`);
    return `\u0000${codes.length - 1}\u0000`;
  });
  html = html
    .replace(/\[([^\]\n]+)\]\(((?:[^()\s]|\([^()\s]*\))+)\)/g, (_, label: string, url: string) => `<span class="md-link" title="${url}">${label}</span>`)
    .replace(/(\*\*|__)(?=\S)(.+?)(?<=\S)\1/g, "<strong>$2</strong>")
    .replace(/~~(?=\S)(.+?)(?<=\S)~~/g, "<del>$1</del>")
    .replace(/(^|[^\w*])\*(?=\S)([^*\n]+?)(?<=\S)\*(?!\*)/g, "$1<em>$2</em>")
    .replace(/(^|[^\w_])_(?=\S)([^_\n]+?)(?<=\S)_(?![\w_])/g, "$1<em>$2</em>");
  return html.replace(/\u0000(\d+)\u0000/g, (_, index: string) => codes[Number(index)]);
}

interface ListItem {
  indent: number;
  ordered: boolean;
  start: number;
  html: string;
}

const listLine = /^(\s*)([-*+]|(\d{1,9})[.)])\s+(.*)$/;

function renderList(items: ListItem[]): string {
  let html = "";
  const stack: { indent: number; ordered: boolean }[] = [];
  for (const item of items) {
    while (stack.length && item.indent < stack[stack.length - 1].indent) {
      html += stack.pop()!.ordered ? "</li></ol>" : "</li></ul>";
    }
    const top = stack[stack.length - 1];
    if (!top || item.indent > top.indent) {
      stack.push({ indent: item.indent, ordered: item.ordered });
      html += item.ordered ? (item.start !== 1 ? `<ol start="${item.start}">` : "<ol>") : "<ul>";
    } else {
      html += "</li>";
    }
    html += `<li>${item.html}`;
  }
  while (stack.length) html += stack.pop()!.ordered ? "</li></ol>" : "</li></ul>";
  return html;
}

function renderTable(rows: string[]): string | null {
  const cells = (row: string) => row.trim().replace(/^\|/, "").replace(/\|$/, "").split("|").map((cell) => cell.trim());
  if (rows.length < 2 || !/^\s*\|?\s*:?-{2,}:?\s*(\|\s*:?-{2,}:?\s*)*\|?\s*$/.test(rows[1])) return null;
  const head = cells(rows[0]).map((cell) => `<th>${renderInline(cell)}</th>`).join("");
  const body = rows.slice(2).map((row) => `<tr>${cells(row).map((cell) => `<td>${renderInline(cell)}</td>`).join("")}</tr>`).join("");
  return `<div class="md-table"><table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table></div>`;
}

/** Renders Markdown to escaped, link-free HTML for display. */
export function renderMarkdown(text: string): string {
  const lines = text.replace(/\r\n?/g, "\n").split("\n");
  const out: string[] = [];
  let index = 0;

  while (index < lines.length) {
    const line = lines[index];

    if (!line.trim()) {
      index += 1;
      continue;
    }

    const fence = line.match(/^ {0,3}(```|~~~)\s*([\w+-]*)/);
    if (fence) {
      const body: string[] = [];
      index += 1;
      while (index < lines.length && !lines[index].trimStart().startsWith(fence[1])) body.push(lines[index++]);
      index += 1; // Skip the closing fence, or run past the end while still streaming.
      out.push(`<pre dir="ltr"><code>${escapeHtml(body.join("\n"))}</code></pre>`);
      continue;
    }

    const heading = line.match(/^ {0,3}(#{1,6})\s+(.*?)\s*#*\s*$/);
    if (heading) {
      const level = heading[1].length;
      out.push(`<h${level}>${renderInline(heading[2])}</h${level}>`);
      index += 1;
      continue;
    }

    if (/^ {0,3}([-*_])(\s*\1){2,}\s*$/.test(line)) {
      out.push("<hr>");
      index += 1;
      continue;
    }

    if (/^ {0,3}>/.test(line)) {
      const quoted: string[] = [];
      while (index < lines.length && /^ {0,3}>/.test(lines[index])) quoted.push(lines[index++].replace(/^ {0,3}>\s?/, ""));
      out.push(`<blockquote>${renderMarkdown(quoted.join("\n"))}</blockquote>`);
      continue;
    }

    if (/^\s*\|.*\|\s*$/.test(line)) {
      const rows: string[] = [];
      while (index < lines.length && /^\s*\|.*\|\s*$/.test(lines[index])) rows.push(lines[index++]);
      out.push(renderTable(rows) ?? `<p>${rows.map(renderInline).join("<br>")}</p>`);
      continue;
    }

    if (listLine.test(line)) {
      const items: ListItem[] = [];
      while (index < lines.length) {
        const match = lines[index].match(listLine);
        if (match) {
          let content = renderInline(match[4]);
          const task = match[4].match(/^\[([ xX])\]\s+(.*)$/);
          if (task) {
            const done = task[1] !== " ";
            content = `<span class="md-task${done ? " done" : ""}" aria-label="${done ? "Done" : "To do"}"></span>${renderInline(task[2])}`;
          }
          items.push({
            indent: match[1].replace(/\t/g, "    ").length,
            ordered: match[3] !== undefined,
            start: match[3] !== undefined ? Number(match[3]) : 1,
            html: content,
          });
          index += 1;
        } else if (lines[index].trim() && /^\s+/.test(lines[index]) && items.length) {
          // An indented line continues the previous item.
          items[items.length - 1].html += `<br>${renderInline(lines[index].trim())}`;
          index += 1;
        } else {
          break;
        }
      }
      out.push(renderList(items));
      continue;
    }

    const paragraph: string[] = [];
    while (index < lines.length && lines[index].trim() && (paragraph.length === 0 || !blockStart.test(lines[index]))) {
      paragraph.push(lines[index++]);
    }
    out.push(`<p>${paragraph.map((part) => renderInline(part.trim())).join("<br>")}</p>`);
  }

  return out.join("");
}
