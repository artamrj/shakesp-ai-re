import assert from "node:assert/strict";
import { test } from "node:test";
import { looksLikeMarkdown, renderInline, renderMarkdown } from "../src/markdown.ts";

test("detects Markdown structure but not ordinary prose", () => {
  assert.ok(looksLikeMarkdown("# Title\nText"));
  assert.ok(looksLikeMarkdown("- one\n- two"));
  assert.ok(looksLikeMarkdown("Use **bold** here"));
  assert.ok(looksLikeMarkdown("```\ncode\n```"));
  assert.ok(!looksLikeMarkdown("Just a sentence - with a dash."));
  assert.ok(!looksLikeMarkdown("I paid 5 * 3 dollars."));
  assert.ok(!looksLikeMarkdown("سلام، حال شما چطور است؟"));
});

test("renders headings, lists, and paragraphs", () => {
  assert.equal(
    renderMarkdown("# عنوان\n\nمتن اول\nمتن دوم\n\n- یک\n- دو"),
    "<h1>عنوان</h1><p>متن اول<br>متن دوم</p><ul><li>یک</li><li>دو</li></ul>",
  );
});

test("nests lists by indentation and keeps ordered starts", () => {
  assert.equal(
    renderMarkdown("3. three\n4. four\n   - inner\n5. five"),
    '<ol start="3"><li>three</li><li>four<ul><li>inner</li></ul></li><li>five</li></ol>',
  );
});

test("renders task lists, quotes, rules, and tables", () => {
  assert.equal(
    renderMarkdown("- [x] done\n- [ ] todo"),
    '<ul><li><span class="md-task done" aria-label="Done"></span>done</li><li><span class="md-task" aria-label="To do"></span>todo</li></ul>',
  );
  assert.equal(renderMarkdown("> quoted\n\n---"), "<blockquote><p>quoted</p></blockquote><hr>");
  assert.equal(
    renderMarkdown("| a | b |\n|---|---|\n| 1 | 2 |"),
    '<div class="md-table"><table><thead><tr><th>a</th><th>b</th></tr></thead><tbody><tr><td>1</td><td>2</td></tr></tbody></table></div>',
  );
});

test("formats inline marks and leaves code untouched", () => {
  assert.equal(renderInline("**bold** and *it* and ~~gone~~"), "<strong>bold</strong> and <em>it</em> and <del>gone</del>");
  assert.equal(renderInline("`**not bold**`"), "<code>**not bold**</code>");
  assert.equal(renderInline("snake_case_name stays"), "snake_case_name stays");
});

test("escapes HTML and never creates real links", () => {
  const html = renderMarkdown('<img src=x onerror=alert(1)> [site](javascript:alert(1)) "q"');
  assert.ok(!html.includes("<img"));
  assert.ok(!html.includes("<a"));
  assert.ok(!html.includes("href"));
  assert.ok(html.includes("&lt;img"));
  assert.ok(html.includes('<span class="md-link" title="javascript:alert(1)">site</span>'));
});

test("keeps code blocks verbatim and left-to-right, even while still streaming", () => {
  assert.equal(renderMarkdown("```js\nconst a = <b>;\n```"), '<pre dir="ltr"><code>const a = &lt;b&gt;;</code></pre>');
  assert.equal(renderMarkdown("```\nunfinished"), '<pre dir="ltr"><code>unfinished</code></pre>');
});
