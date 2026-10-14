import assert from "node:assert/strict";
import { test } from "node:test";
import { compareText, visibleWhitespace } from "../src/text-diff.ts";

function verify(original, corrected) {
  const result = compareText(original, corrected);
  assert.ok(result, "a normal selection should have a comparison");
  assert.equal(result.parts.filter((part) => part.kind !== "added").map((part) => part.value).join(""), original);
  assert.equal(result.parts.filter((part) => part.kind !== "removed").map((part) => part.value).join(""), corrected);
  return result;
}

test("counts a replacement as one edit and keeps separate edits distinct", () => {
  const result = verify("She don’t know where the files is.", "She doesn’t know where the files are.");
  assert.equal(result.changeCount, 2);
  assert.deepEqual(result.parts.filter((part) => part.kind === "removed").map((part) => part.value), ["don’t", "is"]);
  assert.deepEqual(result.parts.filter((part) => part.kind === "added").map((part) => part.value), ["doesn’t", "are"]);
});

test("recognizes unchanged text, including formatting and empty strings", () => {
  for (const text of ["", "Already correct.\n\nSecond paragraph.  ", "سلام 👩🏽‍💻", "你好世界。", "e\u0301"]) {
    assert.equal(verify(text, text).changeCount, 0);
  }
});

test("preserves exact spaces, tabs, line endings, punctuation and HTML-like text", () => {
  for (const [original, corrected] of [
    ["Hello  world", "Hello world"],
    ["Hello world", "Hello, world!"],
    ["First\r\n\r\n\tSecond  ", "First.\r\n\r\n\tSecond.  "],
    ["First\nSecond", "First\n\nSecond"],
    ["<script>alert('x')</script>", "<script>alert('y')</script>"],
    ["", "New text"],
    ["Old text", ""],
  ]) {
    assert.ok(verify(original, corrected).changeCount > 0);
  }
  assert.equal(visibleWhitespace(" \t\r\n\n"), "·⇥↵\n↵\n");
  assert.equal(visibleWhitespace("\u00a0\u2003\u2028"), "··↵\n");
});

test("handles RTL, CJK, mixed languages, combining marks and emoji", () => {
  for (const [original, corrected] of [
    ["سلام خوبی", "سلام، خوبی؟"],
    ["你好世界。", "你好，世界。"],
    ["今日は良い天気です", "今日は良い天気です。"],
    ["مرحبا team this are ready 👩🏽‍💻", "مرحبا team this is ready 👩🏽‍💻"],
    ["Cafe\u0301 are open 🏳️‍🌈", "Cafe\u0301 is open 🏳️‍🌈"],
    ["Great 👍🏽", "Great 👍🏻!"],
  ]) verify(original, corrected);
});

test("falls back to the clean result for oversized or heavily rewritten text", () => {
  assert.equal(compareText("a".repeat(100_001), "b"), null);
  assert.equal(compareText("a ".repeat(6_000), "b ".repeat(6_000)), null);
  assert.equal(compareText("a ".repeat(800), "b ".repeat(800)), null);
});

test("can reconstruct both inputs across varied token edits", () => {
  const tokens = ["word", " ", "\n", "\t", ".", "،", "世界", "سلام", "👩🏽‍💻", "e\u0301"];
  let seed = 42;
  const random = (max) => {
    seed = (Math.imul(seed, 1664525) + 1013904223) >>> 0;
    return seed % max;
  };
  for (let sample = 0; sample < 200; sample += 1) {
    const before = Array.from({ length: 20 }, () => tokens[random(tokens.length)]);
    const after = [...before];
    after.splice(random(after.length), random(3), tokens[random(tokens.length)]);
    verify(before.join(""), after.join(""));
  }
});

test("older webviews without Intl.Segmenter preserve exact text", async () => {
  const nativeSegmenter = Intl.Segmenter;
  try {
    Intl.Segmenter = undefined;
    const { compareText: fallbackCompare } = await import("../src/text-diff.ts?without-segmenter");
    const original = "سلام  世界 👩🏽‍💻\r\nCafe\u0301 are open";
    const corrected = "سلام 世界 👩🏽‍💻\r\nCafe\u0301 is open.";
    const result = fallbackCompare(original, corrected);
    assert.ok(result);
    assert.equal(result.parts.filter((part) => part.kind !== "added").map((part) => part.value).join(""), original);
    assert.equal(result.parts.filter((part) => part.kind !== "removed").map((part) => part.value).join(""), corrected);
  } finally {
    Intl.Segmenter = nativeSegmenter;
  }
});
