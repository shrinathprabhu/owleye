import { readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";
import { verifyNodeVersion } from "./common.mjs";

await verifyNodeVersion();

const { packageManager } = JSON.parse(
  await readFile(new URL("../package.json", import.meta.url), "utf8"),
);
const expected = /^pnpm@(\d+\.\d+\.\d+)$/.exec(packageManager ?? "")?.[1];
if (!expected) throw new Error("package.json must pin one exact pnpm version.");
const actual = execFileSync("pnpm", ["--version"], { encoding: "utf8" }).trim();
if (actual !== expected)
  throw new Error(`pnpm ${expected} is required; found ${actual}.`);
console.log(`Verified pnpm ${expected}`);
