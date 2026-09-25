import test from "node:test";
import assert from "node:assert/strict";
import { mkdtemp, cp, mkdir, writeFile, readFile, rm } from "node:fs/promises";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { pathToFileURL } from "node:url";
import { createServer } from "node:net";
import { root } from "./common.mjs";

test(
  "deploy starts a combined server and restores it after binary or backup failure",
  {
    skip:
      !process.env.OWLEYE_TEST_API_BINARY ||
      !process.env.OWLEYE_TEST_CLICKHOUSE_URL,
  },
  async () => {
    const dir = await mkdtemp(join(tmpdir(), "owleye-deploy-"));
    const oldPath = process.env.PATH;
    let apiPid;
    try {
      await mkdir(join(dir, "scripts"));
      await cp(
        join(root, "scripts/common.mjs"),
        join(dir, "scripts/common.mjs"),
      );
      const ops = await import(pathToFileURL(join(dir, "scripts/common.mjs")));
      await mkdir(ops.data, { mode: 0o700 });
      const release = join(ops.data, "releases", "first");
      await mkdir(join(release, "console"), { recursive: true });
      await writeFile(
        join(release, "console", "index.html"),
        "<html><body>OwlEye deployment test</body></html>",
      );
      await cp(process.env.OWLEYE_TEST_API_BINARY, join(release, "owleye-api"));
      const port = await new Promise((resolve, reject) => {
        const s = createServer();
        s.once("error", reject);
        s.listen(0, "127.0.0.1", () => {
          const port = s.address().port;
          s.close(() => resolve(port));
        });
      });
      const settings = {
        OWLEYE_API_ADDR: `127.0.0.1:${port}`,
        OWLEYE_APP_URL: `http://127.0.0.1:${port}`,
        SQLITE_URL: `sqlite://${join(ops.data, "owleye.sqlite")}`,
        CLICKHOUSE_URL: process.env.OWLEYE_TEST_CLICKHOUSE_URL,
        OWLEYE_HASH_SALT: "isolated-deployment-test-salt-0000111122223333",
      };
      const record = async () =>
        JSON.parse(await readFile(join(ops.data, "running.json"), "utf8"));
      await ops.deploy(settings, release);
      apiPid = (await record()).pid;
      assert.equal(
        (await fetch(settings.OWLEYE_APP_URL + "/health/ready")).status,
        200,
      );
      await assert.rejects(
        ops.deploy(settings, join(ops.data, "missing")),
        /ENOENT/,
      );
      let restarted = await record();
      apiPid = restarted.pid;
      assert.equal(restarted.release, release);
      assert.equal((await fetch(settings.OWLEYE_APP_URL + "/")).status, 200);
      const bin = join(dir, "bin");
      await mkdir(bin);
      await writeFile(join(bin, "sqlite3"), "#!/bin/sh\nexit 23\n", {
        mode: 0o700,
      });
      process.env.PATH = bin + ":" + oldPath;
      await assert.rejects(ops.deploy(settings, release), /sqlite3 failed/);
      restarted = await record();
      apiPid = restarted.pid;
      assert.equal(restarted.release, release);
      assert.equal(
        (await fetch(settings.OWLEYE_APP_URL + "/health/ready")).status,
        200,
      );
    } finally {
      process.env.PATH = oldPath;
      if (apiPid) {
        try {
          process.kill(apiPid, "SIGTERM");
        } catch {}
      }
      await rm(dir, { recursive: true, force: true });
    }
  },
);
