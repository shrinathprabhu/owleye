import assert from "node:assert/strict";
import { createHash } from "node:crypto";
import { existsSync, readFileSync, readdirSync, statSync } from "node:fs";
import { test } from "node:test";

const root = new URL("../", import.meta.url);
const manifest = JSON.parse(
  readFileSync(new URL("package.json", root), "utf8"),
);
const read = (path) => readFileSync(new URL(path, root));

function exportTargets(value) {
  return typeof value === "string"
    ? [value]
    : Object.values(value).flatMap(exportTargets);
}

test("declared exports and versioned CDN documentation resolve to release files", () => {
  const targets = exportTargets(manifest.exports);
  for (const target of targets) {
    assert.ok(target.startsWith("./"), `nonlocal export: ${target}`);
    assert.ok(
      statSync(new URL(target, root)).isFile(),
      `missing export: ${target}`,
    );
    assert.ok(
      target === "./package.json" ||
        manifest.files.some(
          (included) =>
            target.startsWith(`./${included}/`) || target === `./${included}`,
        ),
      `export excluded from release files: ${target}`,
    );
  }

  const docs = ["README.md"];
  const websiteDoc = "../../apps/website/src/content/docs/cdn.md";
  if (existsSync(new URL(websiteDoc, root))) docs.push(websiteDoc);
  for (const doc of docs) {
    const urls = [
      ...read(doc)
        .toString("utf8")
        .matchAll(
          /https:\/\/cdn\.jsdelivr\.net\/npm\/@owleye\/analytics@([^/\s]+)\/([^\s"<>`]+)/g,
        ),
    ];
    assert.ok(urls.length > 0, `no delivery URL checked in ${doc}`);
    for (const [, version, path] of urls) {
      assert.equal(version, manifest.version, `stale CDN version in ${doc}`);
      assert.ok(
        targets.includes(`./${path}`),
        `undocumented package target: ${path}`,
      );
      assert.ok(
        statSync(new URL(path, root)).isFile(),
        `missing CDN file: ${path}`,
      );
    }
  }
});

test("integrity manifest matches every standalone CDN export exactly", () => {
  const integrity = JSON.parse(read("dist/integrity.json"));
  assert.equal(integrity.name, manifest.name);
  assert.equal(integrity.version, manifest.version);
  const files = Object.entries(manifest.exports)
    .filter(([key]) => key === "./cdn" || key.startsWith("./cdn/"))
    .map(([, path]) => path.replace("./dist/", ""));
  assert.deepEqual(Object.keys(integrity.files).sort(), files.sort());
  for (const file of files) {
    const bytes = read(`dist/${file}`);
    assert.equal(integrity.files[file].bytes, bytes.length, file);
    assert.equal(
      integrity.files[file].integrity,
      `sha384-${createHash("sha384").update(bytes).digest("base64")}`,
      file,
    );
  }
});

test("published JavaScript and declaration source-map references remain intact", () => {
  let checked = 0;
  function visit(directory) {
    for (const entry of readdirSync(new URL(directory, root), {
      withFileTypes: true,
    })) {
      const path = `${directory}/${entry.name}`;
      if (entry.isDirectory()) visit(path);
      else if (/\.(js|d\.ts)$/.test(entry.name)) {
        const source = read(path).toString("utf8");
        const reference = source.match(/\/\/# sourceMappingURL=(\S+)/);
        assert.ok(reference, `missing source-map reference: ${path}`);
        const map = JSON.parse(
          readFileSync(new URL(reference[1], new URL(path, root)), "utf8"),
        );
        assert.equal(map.version, 3, path);
        assert.ok(Array.isArray(map.sources), path);
        checked++;
      }
    }
  }
  visit("dist");
  assert.ok(checked > 0);
});
