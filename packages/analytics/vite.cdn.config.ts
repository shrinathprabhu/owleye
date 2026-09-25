import { resolve } from "node:path";
import { defineConfig } from "vite";
import { version } from "./package.json";

const entries = {
  browser: {
    file: "src/browser.ts",
    output: "owleye.analytics.iife.js",
  },
  "browser-rules": {
    file: "src/browser-rules.ts",
    output: "owleye.rules.iife.js",
  },
  "browser-performance": {
    file: "src/browser-performance.ts",
    output: "owleye.performance.iife.js",
  },
} as const;

const entryName = (process.env.OWLEYE_CDN_ENTRY ??
  "browser") as keyof typeof entries;
const entry = entries[entryName] ?? entries.browser;

export default defineConfig({
  define: { __OWLEYE_SDK_VERSION__: JSON.stringify(version) },
  build: {
    emptyOutDir: false,
    lib: {
      entry: resolve(__dirname, entry.file),
      formats: ["iife"],
      name: process.env.OWLEYE_CDN_NAME ?? "OwlEyeAnalytics",
    },
    minify: "oxc",
    rolldownOptions: {
      external: [],
      output: {
        entryFileNames: entry.output,
      },
      treeshake: true,
    },
    sourcemap: true,
    target: "es2020",
  },
});
