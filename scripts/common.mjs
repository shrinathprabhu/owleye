import { spawn, spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { dirname, join, resolve } from "node:path";
import {
  mkdir,
  readFile,
  writeFile,
  open,
  unlink,
  cp,
  access,
} from "node:fs/promises";
import { createServer } from "node:net";
export const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
export const data = join(root, ".owleye");
export const envPath = join(data, "config.json");
export function run(command, args = [], options = {}) {
  return new Promise((yes, no) => {
    const child = spawn(command, args, {
      cwd: root,
      stdio: "inherit",
      ...options,
    });
    child.on("error", no);
    child.on("exit", (code, signal) =>
      code === 0
        ? yes()
        : no(new Error(`${command} failed (${signal ?? code})`)),
    );
  });
}
export async function prerequisites() {
  if (!["linux", "darwin"].includes(process.platform))
    throw new Error("Use Linux, macOS, or WSL2.");
  const missing = [];
  for (const command of ["pnpm", "cargo", "rustc", "sqlite3"])
    if (spawnSync(command, ["--version"], { stdio: "ignore" }).status !== 0)
      missing.push(command);
  if (
    spawnSync("clickhouse", ["--version"], { stdio: "ignore" }).status !== 0 &&
    spawnSync("clickhouse-client", ["--version"], { stdio: "ignore" })
      .status !== 0
  )
    missing.push("clickhouse (or clickhouse-client)");
  if (missing.length)
    throw new Error(
      `Install prerequisites before setup: ${missing.join(", ")}. No system packages were installed.\nInstallation instructions: docs/SELF_HOSTING.md#installing-prerequisites\nhttps://github.com/shrinathprabhu/owleye/blob/HEAD/docs/SELF_HOSTING.md#installing-prerequisites\nAfter installing, reopen your terminal and run ./setup.sh --check.`,
    );
  if (
    Number(process.versions.node.split(".")[0]) !== 26 ||
    Number(process.versions.node.split(".")[1]) < 2
  )
    throw new Error(
      "Install Node.js 26.2 or newer in the Node 26 release line: https://nodejs.org/en/download. Then run ./setup.sh --check.",
    );
  const rust = spawnSync("rustc", ["--version"], {
    encoding: "utf8",
  }).stdout.match(/rustc (\d+)\.(\d+)/);
  if (
    !rust ||
    Number(rust[1]) < 1 ||
    (Number(rust[1]) === 1 && Number(rust[2]) < 95)
  )
    throw new Error(
      "Rust 1.95 or newer is required: https://rust-lang.org/tools/install/. Then run ./setup.sh --check.",
    );
  const pnpm = spawnSync("pnpm", ["--version"], {
    encoding: "utf8",
  }).stdout.trim();
  if (pnpm !== "10.28.1")
    throw new Error(
      "Install pnpm 10.28.1 (npm install -g pnpm@10.28.1). See https://pnpm.io/10.x/installation. Then run ./setup.sh --check.",
    );
}
export async function config() {
  return JSON.parse(
    await readFile(envPath, "utf8").catch(() => {
      throw new Error("Run ./setup.sh first.");
    }),
  );
}
export async function lock(fn) {
  await mkdir(data, { recursive: true, mode: 0o700 });
  const path = join(data, "deploy.lock");
  let handle;
  try {
    handle = await open(path, "wx", 0o600);
  } catch {
    throw new Error(
      "Another deployment is running. If it crashed, verify the PID in .owleye/deploy.lock before removing the lock.",
    );
  }
  await handle.writeFile(String(process.pid));
  try {
    return await fn();
  } finally {
    await handle.close();
    await unlink(path);
  }
}
export async function save(path, value) {
  await writeFile(path, JSON.stringify(value, null, 2) + "\n", { mode: 0o600 });
}
export async function availablePort(host, port) {
  await new Promise((yes, no) => {
    const server = createServer();
    server.once("error", no);
    server.listen(port, host, () => server.close(yes));
  });
}
export async function build() {
  await run("pnpm", ["install", "--frozen-lockfile"]);
  await run("pnpm", ["--filter", "@owleye/console", "build"]);
  await run("cargo", ["build", "--release", "--locked", "-p", "owleye-api"]);
  const release = join(
    data,
    "releases",
    new Date().toISOString().replace(/[:.]/g, "-"),
  );
  await mkdir(release, { recursive: true, mode: 0o700 });
  await cp(
    join(root, "target/release/owleye-api"),
    join(release, "owleye-api"),
  );
  await cp(
    join(root, "apps/console/.output/public"),
    join(release, "console"),
    { recursive: true },
  );
  return release;
}
function alive(pid) {
  try {
    process.kill(pid, 0);
    return true;
  } catch (e) {
    if (e.code === "ESRCH") return false;
    throw e;
  }
}
async function stop(previous) {
  if (!previous || !alive(previous.pid)) return;
  const actual = spawnSync(
    "ps",
    ["-p", String(previous.pid), "-o", "command="],
    { encoding: "utf8" },
  ).stdout.trim();
  if (actual !== join(previous.release, "owleye-api"))
    throw new Error(
      "Recorded PID belongs to a different process; refusing to stop it.",
    );
  process.kill(previous.pid, "SIGTERM");
  for (let i = 0; i < 120; i++) {
    if (!alive(previous.pid)) return;
    await new Promise((r) => setTimeout(r, 250));
  }
  throw new Error(
    "Server did not stop within 30 seconds. Inspect .owleye/server.log.",
  );
}
export async function start(release, settings) {
  const log = await open(join(data, "server.log"), "a", 0o600);
  let started;
  try {
    const child = spawn(join(release, "owleye-api"), [], {
      cwd: root,
      env: {
        ...process.env,
        ...settings,
        OWLEYE_CONSOLE_DIR: join(release, "console"),
      },
      detached: true,
      stdio: ["ignore", log.fd, log.fd],
    });
    started = await new Promise((yes, no) => {
      child.once("error", no);
      child.once("spawn", () => yes(child.pid));
    });
    child.unref();
  } finally {
    await log.close();
  }
  const record = { pid: started, release };
  await save(join(data, "running.json"), record);
  const address = new URL(`http://${settings.OWLEYE_API_ADDR}`);
  if (address.hostname === "0.0.0.0") address.hostname = "127.0.0.1";
  if (address.hostname === "[::]") address.hostname = "[::1]";
  address.pathname = "/health/ready";
  for (let i = 0; i < 60; i++) {
    if (!alive(started)) break;
    try {
      const response = await fetch(address, {
        signal: AbortSignal.timeout(800),
      });
      if (response.ok) {
        const html = await fetch(new URL("/", address), {
          signal: AbortSignal.timeout(800),
        });
        if (html.ok && (await html.text()).includes("<html")) return record;
      }
    } catch {}
    await new Promise((r) => setTimeout(r, 500));
  }
  await stop(record);
  throw new Error("Server readiness failed. Inspect .owleye/server.log.");
}
export async function deploy(settings, release) {
  const previous = JSON.parse(
    await readFile(join(data, "running.json"), "utf8").catch(() => "null"),
  );
  await stop(previous);
  try {
    // Snapshot while the API is stopped, and restart the previous build on any deployment failure.
    const database = join(data, "owleye.sqlite");
    try {
      await access(database);
      await mkdir(join(data, "backups"), { recursive: true, mode: 0o700 });
      const backup = join(data, "backups", `${Date.now()}.sqlite`);
      await run("sqlite3", [
        database,
        `.backup '${backup.replaceAll("'", "''")}'`,
      ]);
    } catch (e) {
      if (e.code !== "ENOENT") throw e;
    }
    await start(release, settings);
  } catch (e) {
    if (previous) {
      console.error(
        "Deployment failed; attempting to restart the previous build.",
      );
      await start(previous.release, settings).catch((error) =>
        console.error(error.message),
      );
    }
    throw e;
  }
  await save(join(data, "current.json"), { release });
  console.log(
    `OwlEye is running at ${settings.OWLEYE_APP_URL}. Logs: .owleye/server.log`,
  );
}
