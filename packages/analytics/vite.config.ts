import { resolve } from "node:path";
import { defineConfig } from "vite";
import { version } from "./package.json";

export default defineConfig({
  define: { __OWLEYE_SDK_VERSION__: JSON.stringify(version) },
  build: {
    emptyOutDir: false,
    lib: {
      entry: {
        index: resolve(__dirname, "src/index.ts"),
        rules: resolve(__dirname, "src/rules.ts"),
        performance: resolve(__dirname, "src/performance.ts"),
      },
      formats: ["es"],
    },
    minify: "oxc",
    rolldownOptions: {
      external: [],
      treeshake: true,
      output: {
        entryFileNames: "[name].js",
      },
    },
    sourcemap: true,
    target: "es2020",
  },
});
