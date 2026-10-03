import { createHash } from "node:crypto";
import { readFile, writeFile } from "node:fs/promises";

const root = new URL("../", import.meta.url);
const license = await readFile(new URL("LICENSE", root), "utf8");
const { name, version } = JSON.parse(
  await readFile(new URL("package.json", root), "utf8"),
);
const files = [
  "owleye.full.iife.js",
  "owleye.analytics.iife.js",
  "owleye.rules.iife.js",
  "owleye.performance.iife.js",
];
const integrity = {};
for (const file of files) {
  const bytes = await readFile(new URL(`dist/${file}`, root));
  if (!bytes.toString("utf8").includes(license))
    throw new Error(`${file} is missing the SDK license notice.`);
  integrity[file] = {
    bytes: bytes.byteLength,
    integrity: `sha384-${createHash("sha384").update(bytes).digest("base64")}`,
  };
}
await writeFile(
  new URL("dist/integrity.json", root),
  `${JSON.stringify({ name, version, files: integrity }, null, 2)}\n`,
);
console.log("Wrote SHA-384 integrity for all standalone SDK bundles.");
