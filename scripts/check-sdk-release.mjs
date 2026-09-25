import { readFile } from "node:fs/promises";
import { execFileSync } from "node:child_process";

const npmVersion = execFileSync("npm", ["--version"], {
  encoding: "utf8",
}).trim();
const [major, minor, patch] = npmVersion.split(".").map(Number);
if (!(
  major > 11 ||
  (major === 11 && (minor > 5 || (minor === 5 && patch >= 1)))
))
  throw new Error(
    `npm 11.5.1 or newer is required for trusted publishing; found ${npmVersion}.`,
  );
const pkg = JSON.parse(
  await readFile(
    new URL("../packages/analytics/package.json", import.meta.url),
  ),
);
const tag = process.env.RELEASE_TAG;
if (tag !== `v${pkg.version}` && tag !== `@owleye/analytics@${pkg.version}`)
  throw new Error(
    `Release tag must be v${pkg.version} or @owleye/analytics@${pkg.version}; update package.json before tagging.`,
  );
if (
  pkg.name !== "@owleye/analytics" ||
  pkg.repository.url !== "git+https://github.com/shrinathprabhu/owleye.git" ||
  pkg.repository.directory !== "packages/analytics"
)
  throw new Error("Unexpected package identity or source.");
const readme = await readFile(
  new URL("../packages/analytics/README.md", import.meta.url),
  "utf8",
);
if (!readme.startsWith("# @owleye/analytics\n"))
  throw new Error("The published package must include the SDK README.");
console.log(`Verified ${pkg.name}@${pkg.version}`);
