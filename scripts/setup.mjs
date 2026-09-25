import { createInterface } from "node:readline/promises";
import { stdin, stdout } from "node:process";
import { randomBytes } from "node:crypto";
import { lstat, stat, readFile } from "node:fs/promises";
import { join, resolve } from "node:path";
import { spawn } from "node:child_process";
import {
  config,
  root,
  data,
  envPath,
  prerequisites,
  lock,
  save,
  build,
  deploy,
  availablePort,
} from "./common.mjs";
async function ask(label, fallback = "") {
  const rl = createInterface({ input: stdin, output: stdout });
  try {
    return (
      (
        await rl.question(`${label}${fallback ? ` [${fallback}]` : ""}: `)
      ).trim() || fallback
    );
  } finally {
    rl.close();
  }
}
async function secret(label) {
  if (!stdin.isTTY)
    throw new Error(
      "Setup requires an interactive terminal for hidden passwords.",
    );
  stdout.write(`${label}: `);
  stdin.setRawMode(true);
  stdin.resume();
  let value = "";
  return new Promise((yes, no) => {
    const end = () => {
      stdin.off("data", receive);
      stdin.setRawMode(false);
      stdin.pause();
      stdout.write("\n");
    };
    const receive = (buffer) => {
      for (const char of buffer.toString()) {
        if (char === "\u0003") {
          end();
          no(new Error("Cancelled"));
          return;
        }
        if (char === "\r" || char === "\n") {
          end();
          yes(value);
          return;
        }
        if (char === "\u007f") {
          value = [...value].slice(0, -1).join("");
        } else if (char >= " ") {
          value += char;
        }
      }
    };
    stdin.on("data", receive);
  });
}
async function assertNotConfigured() {
  try {
    await lstat(envPath);
  } catch (error) {
    if (error.code === "ENOENT") return;
    throw error;
  }
  throw new Error(
    "Already configured. Nothing was reset or restarted. Run ./redeploy.sh; setup never overwrites credentials.",
  );
}
async function setup() {
  if (
    !process.argv.includes("--check") &&
    !process.argv.includes("--recover-admin")
  )
    await assertNotConfigured();
  await prerequisites();
  if (process.argv.includes("--check")) {
    console.log("All prerequisites found.");
    return;
  }
  await lock(async () => {
    if (process.argv.includes("--recover-admin")) {
      const settings = await config();
      const { release } = JSON.parse(
        await readFile(join(data, "current.json"), "utf8"),
      );
      const email = await ask("Administrator email to recover");
      const password = await secret(
        "New administrator password (at least 8 characters)",
      );
      if ([...password].length < 8)
        throw new Error("Password must contain at least 8 characters.");
      if (password !== (await secret("Confirm password")))
        throw new Error("Passwords do not match.");
      await new Promise((yes, no) => {
        const child = spawn(join(release, "owleye-api"), ["recover-admin"], {
          cwd: root,
          env: { ...process.env, ...settings },
          stdio: ["pipe", "inherit", "inherit"],
        });
        child.on("error", no);
        child.on("exit", (code) =>
          code === 0 ? yes() : no(new Error("Administrator recovery failed.")),
        );
        child.stdin.end(JSON.stringify({ email, password }));
      });
      console.log(
        "Administrator recovered. Existing sessions and authenticator setup were revoked. Sign in and configure your authenticator again if needed.",
      );
      return;
    }
    // Recheck under the lock in case another setup finished after the first check.
    await assertNotConfigured();
    console.log(
      "Recommended minimum: 4 vCPU, 8 GB RAM, 20 GB free storage. This is guidance, not a setup restriction.",
    );
    const port = Number(await ask("Server port", "8527"));
    if (!Number.isInteger(port) || port < 1024 || port > 65535)
      throw new Error("Choose a port between 1024 and 65535.");
    const host = await ask("Listen address", "127.0.0.1");
    if (!/^[\d.:a-fA-F]+$/.test(host))
      throw new Error("Listen address must be an IP address.");
    await availablePort(host, port);
    const app = new URL(
      await ask(
        "Public Console URL (same origin for APIs)",
        `http://localhost:${port}`,
      ),
    );
    if (
      !["http:", "https:"].includes(app.protocol) ||
      app.pathname !== "/" ||
      app.search ||
      app.hash ||
      app.username ||
      app.password
    )
      throw new Error(
        "Console URL must be an HTTP(S) origin without a path or credentials.",
      );
    const clickhouse = new URL(
      await ask(
        "ClickHouse HTTP URL (dedicated database must already exist)",
        "http://127.0.0.1:8123/?database=owleye",
      ),
    );
    if (!["http:", "https:"].includes(clickhouse.protocol))
      throw new Error("ClickHouse URL must use HTTP(S).");
    clickhouse.username = await ask("ClickHouse user", "default");
    clickhouse.password = await secret("ClickHouse password (optional)");
    const probe = new URL(clickhouse);
    const auth = Buffer.from(
      `${decodeURIComponent(probe.username)}:${decodeURIComponent(probe.password)}`,
    ).toString("base64");
    probe.username = probe.password = "";
    const database =
      probe.searchParams.get("database") ||
      decodeURIComponent(probe.pathname.slice(1)) ||
      "default";
    probe.pathname = "/";
    probe.searchParams.set("database", database);
    clickhouse.pathname = "/";
    clickhouse.searchParams.set("database", database);
    const response = await fetch(probe, {
      method: "POST",
      headers: { Authorization: `Basic ${auth}` },
      body: "SELECT 1",
      signal: AbortSignal.timeout(10000),
    });
    if (!response.ok || !(await response.text()).trim().startsWith("1"))
      throw new Error(
        "Cannot query the ClickHouse database. Verify the database, user, and credentials before setup.",
      );
    const email = await ask("Administrator email");
    if (!/^[^\s@]+@[^\s@]+\.[^\s@]+$/.test(email))
      throw new Error("Enter a valid administrator email address.");
    const password = await secret(
      "Administrator password (at least 8 characters)",
    );
    if ([...password].length < 8)
      throw new Error("Password must contain at least 8 characters.");
    if (password !== (await secret("Confirm administrator password")))
      throw new Error("Passwords do not match.");
    let geo = await ask("Optional MaxMind GeoLite2-City .mmdb path");
    if (geo) {
      geo = resolve(geo);
      if (!(await stat(geo)).isFile())
        throw new Error("MaxMind path must be a readable .mmdb file.");
    } else
      console.log(
        "Location data cannot be tracked until MaxMind GeoIP is configured.",
      );
    const key = await secret(
      "Optional OpenRouter API key (leave empty to disable AI)",
    );
    const model = key ? await ask("OpenRouter model", "z-ai/glm-5.2") : "";
    const proxies = await ask(
      "Trusted reverse-proxy CIDRs, comma separated (optional)",
    );
    const loopback = await ask(
      "Allow SDK events from localhost during development? yes/no",
      "no",
    );
    const settings = {
      OWLEYE_APP_URL: app.origin,
      OWLEYE_API_ADDR: `${host.includes(":") ? `[${host}]` : host}:${port}`,
      CLICKHOUSE_URL: clickhouse.toString(),
      SQLITE_URL: `sqlite://${join(data, "owleye.sqlite")}`,
      OWLEYE_HASH_SALT: randomBytes(32).toString("hex"),
      OWLEYE_MAXMIND_DB: geo,
      OWLEYE_TRUSTED_PROXIES: proxies,
      OWLEYE_ALLOW_LOOPBACK_ORIGINS: loopback === "yes" ? "true" : "false",
      RUST_LOG: "owleye_api=info",
    };
    if (key) {
      settings.OPENROUTER_API_KEY = key;
      settings.OWLEYE_AI_MODEL = model;
    }
    const aiClickhouse = key
      ? await ask(
          "Optional restricted read-only ClickHouse URL for AI (Enter to use main connection)",
        )
      : "";
    if (aiClickhouse) settings.OWLEYE_AI_CLICKHOUSE_URL = aiClickhouse;
    const release = await build();
    await new Promise((yes, no) => {
      const child = spawn(join(release, "owleye-api"), ["bootstrap"], {
        cwd: root,
        env: { ...process.env, ...settings },
        stdio: ["pipe", "inherit", "inherit"],
      });
      child.on("error", no);
      child.on("exit", (code) =>
        code === 0 ? yes() : no(new Error("Administrator bootstrap failed.")),
      );
      child.stdin.end(JSON.stringify({ email, password }));
    });
    await save(envPath, settings);
    await deploy(settings, release);
  });
}
setup().catch((error) => {
  console.error(error.message);
  process.exitCode = 1;
});
