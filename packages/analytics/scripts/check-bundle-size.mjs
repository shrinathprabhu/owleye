import { readFile } from "node:fs/promises";
import { gzipSync } from "node:zlib";
import { build } from "vite";
import { fileURLToPath } from "node:url";

// Measure what an ESM consumer actually downloads, including shared chunks.
// Entry-file-only budgets can hide most of the core SDK in a separate chunk.
async function consumerBundle(source) {
  const entry = fileURLToPath(new URL("../size-consumer.js", import.meta.url));
  const result = await build({
    configFile: false,
    logLevel: "silent",
    plugins: [
      {
        name: "owleye-size-consumer",
        resolveId(id) {
          if (id === entry) return entry;
        },
        load(id) {
          if (id === entry) return source;
        },
      },
    ],
    build: {
      write: false,
      minify: "esbuild",
      target: "es2020",
      lib: { entry, formats: ["es"] },
    },
  });
  return (Array.isArray(result) ? result : [result])
    .flatMap((bundle) => bundle.output)
    .filter((output) => output.type === "chunk")
    .map((output) => output.code)
    .join("\n");
}

// Includes the shared-instance registry and bounded delivery lifecycle,
// fail-safe API boundaries, and bounded diagnostics.
// Includes routing controls, configuration conflict checks and bounded acknowledgement diagnostics.
// Standalone CDN budgets include 800 bytes of gzip allowance for the full MIT notice.
const budgets = new Map([
  ["dist/owleye.full.iife.js", 13_100],
  ["dist/index.js", 1_600],
  ["dist/rules.js", 6_500],
  ["dist/owleye.analytics.iife.js", 7_400],
  ["dist/owleye.rules.iife.js", 11_200],
  ["dist/owleye.performance.iife.js", 7_000],
]);

const formatBytes = (bytes) => `${(bytes / 1_024).toFixed(2)} KiB`;

const measurements = await Promise.all(
  [...budgets].map(async ([file, budget]) => {
    const source = await readFile(new URL(`../${file}`, import.meta.url));
    return { file, budget, size: gzipSync(source, { level: 9 }).byteLength };
  }),
);

for (const [name, symbol, budget] of [
  ["@owleye/analytics", "useAnalytics", 7_000],
  ["@owleye/analytics/rules", "trackRules", 11_100],
  ["@owleye/analytics/performance", "trackPerf", 6_700],
]) {
  const code = await consumerBundle(`export * from "${name}";`);
  measurements.push({
    file: `${name} (complete ESM)`,
    budget,
    size: gzipSync(code, { level: 9 }).byteLength,
  });
  const unused = await consumerBundle(
    `import { ${symbol} } from "${name}"; export const marker = 1;`,
  );
  if (unused.length > 100)
    throw new Error(`${name} does not tree-shake when unused`);
  if (
    name === "@owleye/analytics" &&
    /MutationObserver|PerformanceObserver|\/v1\/rules/.test(code)
  )
    throw new Error("Core SDK includes optional rule or performance features");
}

for (const { file, budget, size } of measurements) {
  console.log(`${file}: ${formatBytes(size)} / ${formatBytes(budget)} gzip`);
}

const failures = measurements.filter(({ budget, size }) => size > budget);
if (failures.length > 0) {
  const details = failures
    .map(
      ({ file, budget, size }) =>
        `${file} exceeds its budget by ${formatBytes(size - budget)}`,
    )
    .join("\n");
  throw new Error(`Analytics bundle size budget exceeded:\n${details}`);
}
