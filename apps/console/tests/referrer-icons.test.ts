import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import test from "node:test";
import {
  referrerIconHost,
  referrerIconSource,
} from "../utils/referrerIcons.ts";

test("recognized referrer labels and bounded hosts use fixed local favicons", () => {
  for (const [source, file] of [
    ["Google", "google.ico"],
    ["www.google.co.in.", "google.ico"],
    ["gemini.google.com", "gemini.svg"],
    ["ChatGPT", "chatgpt.svg"],
    ["X", "x.png"],
    ["t.co", "x.png"],
    ["GitHub", "github.ico"],
    ["www.github.com", "github.ico"],
    ["LinkedIn", "linkedin.ico"],
    ["Product Hunt", "producthunt.svg"],
    ["Reddit", "reddit.png"],
    ["Hacker News", "hackernews.ico"],
    ["news.ycombinator.com", "hackernews.ico"],
    ["YouTube", "youtube.ico"],
    [" lowkey.tools ", "lowkey.svg"],
    ["OwlEye", "owleye.svg"],
  ])
    assert.equal(referrerIconSource(source), `/referrer-icons/${file}`, source);
  assert.equal(referrerIconHost("Product Hunt"), "producthunt.com");
  assert.equal(referrerIconHost("www.lowkey.tools"), "lowkey.tools");
  assert.equal(referrerIconHost("Gemini"), "gemini.google.com");
});

test("Direct, private, unknown, spoofed and malformed labels never become requests", () => {
  for (const source of [
    undefined,
    "",
    "Direct",
    "Direct / unknown",
    "Direct or unknown",
    "Other",
    "unknown.example",
    "customer.internal",
    "localhost",
    "127.0.0.1",
    "notgoogle.com",
    "google.com.attacker.example",
    "lowkey.tools.evil.test",
    "https://google.com/private?token=secret",
    "//github.com",
    "google.com:443",
    "user@google.com",
    "google.com/path",
    "google.com#secret",
    "google.com\u0000",
    "<img src=x onerror=alert(1)>",
    'data:image/svg+xml,<svg onload="alert(1)">',
    "javascript:alert(1)",
    "__proto__",
    "constructor",
    "a".repeat(513),
  ]) {
    assert.equal(referrerIconSource(source), undefined, source);
    assert.equal(referrerIconHost(source), undefined, source);
  }
});

test("bundled favicons have provenance, bounded image bytes, and no active SVG content", () => {
  const directory = new URL("../public/referrer-icons/", import.meta.url);
  const manifest = JSON.parse(
    readFileSync(new URL("sources.json", directory), "utf8"),
  );
  for (const icon of manifest.icons) {
    assert.ok(!icon.unavailable, icon.id);
    assert.match(icon.file, /^[a-z0-9-]+\.(?:svg|png|ico)$/);
    assert.match(icon.source, /^https:\/\//);
    const bytes = readFileSync(new URL(icon.file, directory));
    assert.ok(bytes.length <= 65536, icon.file);
    assert.equal(bytes.length, icon.bytes);
    if (icon.file.endsWith(".svg")) {
      const svg = bytes.toString("utf8");
      assert.match(svg, /<svg\b/);
      assert.doesNotMatch(
        svg,
        /<script|<foreignObject|\b(?:href|src)\s*=|<!ENTITY|<!DOCTYPE|\bon\w+\s*=/i,
      );
      assert.doesNotMatch(svg, /@import|url\(\s*['"]?(?:https?:|\/\/|data:)/i);
    } else if (icon.file.endsWith(".png"))
      assert.equal(bytes.subarray(0, 8).toString("hex"), "89504e470d0a1a0a");
    else assert.equal(bytes.subarray(0, 4).toString("hex"), "00000100");
  }
});
