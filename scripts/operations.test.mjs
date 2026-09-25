import test from "node:test";
import assert from "node:assert/strict";
import {
  mkdtemp,
  cp,
  readFile,
  rm,
  mkdir,
  writeFile,
  readdir,
} from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { spawnSync } from "node:child_process";
import { root } from "./common.mjs";
test("archive updates explain how to redeploy without changing files", async () => {
  const dir = await mkdtemp(join(tmpdir(), "owleye-archive-"));
  try {
    await cp(join(root, "scripts"), join(dir, "scripts"), { recursive: true });
    const result = spawnSync(
      process.execPath,
      [join(dir, "scripts/update.mjs")],
      { encoding: "utf8", cwd: dir },
    );
    assert.equal(result.status, 1);
    assert.match(
      result.stderr,
      /Upload the updated files manually and run .\/redeploy.sh/,
    );
    assert.equal(
      spawnSync("git", ["rev-parse", "--show-toplevel"], { cwd: dir }).status,
      128,
    );
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
test("release publishing requires a tag matching the package version", async () => {
  const pkg = JSON.parse(
    await readFile(join(root, "packages/analytics/package.json")),
  );
  for (const tag of [`v${pkg.version}`, `@owleye/analytics@${pkg.version}`]) {
    assert.equal(
      spawnSync(process.execPath, ["scripts/check-sdk-release.mjs"], {
        cwd: root,
        env: { ...process.env, RELEASE_TAG: tag },
      }).status,
      0,
    );
  }
  assert.notEqual(
    spawnSync(process.execPath, ["scripts/check-sdk-release.mjs"], {
      cwd: root,
      env: { ...process.env, RELEASE_TAG: "v0.0.0" },
    }).status,
    0,
  );
});
test("setup reports every missing prerequisite before requesting credentials", async () => {
  const dir = await mkdtemp(join(tmpdir(), "owleye-prereqs-"));
  try {
    await cp(join(root, "scripts"), join(dir, "scripts"), { recursive: true });
    const empty = join(dir, "empty");
    await mkdir(empty);
    const result = spawnSync(
      process.execPath,
      [join(dir, "scripts/setup.mjs")],
      { env: { ...process.env, PATH: empty }, encoding: "utf8" },
    );
    assert.equal(result.status, 1);
    for (const dependency of [
      "pnpm",
      "cargo",
      "rustc",
      "sqlite3",
      "clickhouse",
    ])
      assert.ok(result.stderr.includes(dependency));
    assert.ok(!result.stdout.includes("Administrator"));
    assert.match(result.stderr, /SELF_HOSTING\.md#installing-prerequisites/);
    assert.match(result.stderr, /\.\/setup\.sh --check/);
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});

test("repeated setup preserves existing state and never builds or restarts", async () => {
  const dir = await mkdtemp(join(tmpdir(), "owleye-repeat-setup-"));
  try {
    await cp(join(root, "scripts"), join(dir, "scripts"), { recursive: true });
    const data = join(dir, ".owleye");
    const bin = join(dir, "bin");
    await mkdir(data);
    await mkdir(bin);
    const files = {
      "config.json": JSON.stringify({
        OWLEYE_HASH_SALT: "test-only-existing-salt",
      }),
      "owleye.sqlite": "existing database sentinel",
      "current.json": JSON.stringify({ release: "/existing/release" }),
      "running.json": JSON.stringify({
        pid: 12345,
        release: "/existing/release",
      }),
      "deploy.lock": "existing lock sentinel",
    };
    for (const [name, contents] of Object.entries(files))
      await writeFile(join(data, name), contents);
    const versions = {
      pnpm: "10.28.1",
      cargo: "cargo 1.95.0",
      rustc: "rustc 1.95.0",
      sqlite3: "3.50.0",
      clickhouse: "ClickHouse client version 25.8.0",
    };
    for (const [name, version] of Object.entries(versions))
      await writeFile(
        join(bin, name),
        `#!/bin/sh\nprintf '%s\\n' invoked >> "$OWLEYE_TEST_COMMAND_LOG"\n[ "$#" -eq 1 ] && [ "$1" = '--version' ] || { echo 'Unexpected command' >&2; exit 97; }\nprintf '%s\\n' '${version}'\n`,
        { mode: 0o755 },
      );
    for (const args of [[], [], ["--check"]]) {
      const result = spawnSync(
        process.execPath,
        [join(dir, "scripts/setup.mjs"), ...args],
        {
          cwd: dir,
          env: {
            ...process.env,
            PATH: bin,
            OWLEYE_TEST_COMMAND_LOG: join(dir, "commands.log"),
          },
          encoding: "utf8",
          timeout: 10_000,
        },
      );
      assert.equal(result.error, undefined);
      if (args.length) {
        assert.equal(result.status, 0);
        assert.match(result.stdout, /All prerequisites found/);
      } else {
        assert.equal(result.status, 1);
        assert.match(
          result.stderr,
          /Already configured\. Nothing was reset or restarted/,
        );
        assert.match(result.stderr, /\.\/redeploy\.sh/);
        await assert.rejects(readFile(join(dir, "commands.log")), {
          code: "ENOENT",
        });
      }
      assert.doesNotMatch(result.stdout, /Administrator|Server port/);
      for (const [name, contents] of Object.entries(files))
        assert.equal(await readFile(join(data, name), "utf8"), contents);
      assert.deepEqual((await readdir(data)).sort(), Object.keys(files).sort());
    }
  } finally {
    await rm(dir, { recursive: true, force: true });
  }
});
