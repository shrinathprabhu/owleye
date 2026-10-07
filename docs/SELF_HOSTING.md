# Self-hosting guide

## Prerequisites

Use Linux (recommended), macOS, or WSL2 with a supported native toolchain. Install these before starting setup; the installer reports missing dependencies and does not install system packages:

| Dependency | Required configuration |
| --- | --- |
| Node.js | 26.2 or newer within Node 26; required for setup and Nuxt builds |
| pnpm | 10.28.1 (`npm install -g pnpm@10.28.1`) |
| Rust / Cargo | 1.95 or newer; install through rustup |
| C/C++ build tools | Linux `build-essential` / platform equivalent, or macOS Command Line Tools; SQLite bindings are built with Cargo |
| ClickHouse | Running server with its HTTP endpoint reachable; `clickhouse` or `clickhouse-client` installed |
| SQLite | `sqlite3` CLI; Rust uses embedded SQLite, with no separate SQLite server |
| Git | Required only for cloning and `./update.sh` |

Builds need network access to npm and crates.io. Docker and Caddy are optional. Configure HTTPS with a reverse proxy of your choice for an Internet-facing installation. Bind to loopback behind that proxy and forward the entire domain, including `/v1`, to one port.

Recommended minimum: **4 vCPU / 8 GB RAM / 20 GB free storage**. This is not enforced. For larger workloads, use 8+ vCPU, 16+ GB RAM and larger SSD storage; capacity depends on traffic and how much history you retain. No product quota limits events, apps, users, or AI prompts. Request-size limits, rate limits, and concurrency safeguards still protect the server. Analytics retention is unlimited by default, so monitor disk usage and maintain backups. Uptime history retains 30 days.

Use a dedicated ClickHouse database and user. For example, create an `owleye` database using your ClickHouse administrator account, then grant the application user schema creation/alteration, inserts, reads and deletion rights on that database. Do not point setup at another application's database. Keep ClickHouse and SQLite private to the server/network.

### Installing prerequisites

Use the upstream instructions for your operating system and CPU architecture. These links can recommend newer defaults; use the versions pinned by this checkout (`.nvmrc`, `package.json`, and `rust-toolchain.toml`). OwlEye's scripts check the required CLI tools and versions; they do not install system packages or configure a database service automatically.

| Dependency | Installation instructions |
| --- | --- |
| Node.js | [Official download and installation options](https://nodejs.org/en/download). Select Node **26**, version **26.2 or newer**; the default LTS selection may be a different major. With an existing nvm installation, run `nvm install` and `nvm use` from this checkout. |
| pnpm | After Node is active, run `npm install -g pnpm@10.28.1`. See [pnpm 10 installation](https://pnpm.io/10.x/installation). Avoid an unpinned `latest` install. |
| Rust / Cargo | Follow [rustup installation](https://rust-lang.org/tools/install/), then run `rustup toolchain install 1.95.0 --profile minimal`. This checkout selects its toolchain through `rust-toolchain.toml`. |
| Build tools | Ubuntu/Debian: install `build-essential` through apt. macOS: run `xcode-select --install`; see [Apple Command Line Tools](https://developer.apple.com/documentation/xcode/installing-the-command-line-tools/). Other Linux distributions: install their C/C++ compiler and build-tool packages. |
| ClickHouse | Follow [ClickHouse installation](https://clickhouse.com/docs/get-started/setup/install) for your platform, including the [Debian/Ubuntu server and client guide](https://clickhouse.com/docs/get-started/setup/self-managed/debian-ubuntu). Start the service and create a dedicated database and user before running setup. A working client command alone does not mean the server is ready. |
| SQLite | Ubuntu/Debian: `sudo apt install sqlite3`. On macOS, check `sqlite3 --version` first. Other platforms: use your package manager or the [SQLite command-line tools downloads](https://www.sqlite.org/download.html). No SQLite server needs to be started. |
| Git (optional) | [Git installation](https://git-scm.com/downloads/). Required for cloning and `./update.sh`; source archives can use redeploy instead. |

For example, on Ubuntu/Debian (including an Ubuntu WSL2 installation), the base packages can be installed with:

```sh
sudo apt update
sudo apt install build-essential sqlite3 git ca-certificates curl
```

This command installs only those base packages. Install Node, pnpm, Rust, and ClickHouse separately using the links above. Follow your Node installation's permissions guidance if a global npm install is denied; run OwlEye itself as your regular installation user rather than with sudo. On WSL2, install and run the tools inside the Linux distribution.

After installation, reopen your terminal so updated PATH entries take effect, return to the public checkout, and run:

```sh
./setup.sh --check
./setup.sh
```

`--check` checks CLI availability and the required Node, pnpm, and Rust versions; it does not validate the complete build toolchain, ClickHouse connectivity or credentials. Normal setup checks the ClickHouse connection before collecting administrator credentials, and the build verifies the native toolchain. No configuration or administrator account is created by `--check`.

## Setup

Clone [the repository](https://github.com/shrinathprabhu/owleye) or extract a source archive, enter its directory, then run:

```sh
./setup.sh --check  # dependency check only
./setup.sh
```

Setup collects the port (default **8527**), listen IP (default `127.0.0.1`), public origin, ClickHouse credentials, administrator email and password. Passwords require at least eight characters without complexity rules. They are salted and hashed with Argon2id in SQLite. No registration endpoint exists. Create additional users under **Users**; manage app permissions there. Profile contains password changes and optional authenticator setup. Administrators can reset other users' passwords after reauthentication. Password changes and resets revoke existing sessions.

The optional prompts cover:

| Configuration | Purpose |
| --- | --- |
| `OWLEYE_MAXMIND_DB` | Absolute path to a locally maintained MaxMind GeoLite2-City `.mmdb` file. **Location data cannot be tracked without it.** Obtain and update the file using your own MaxMind account; license keys are not needed by the API. Redeploy after changing the file. |
| `OPENROUTER_API_KEY` | Enables AI. Operator/provider charges may apply; OwlEye imposes no prompt allowance. Without a key, Console explicitly reports AI unavailable. |
| `OWLEYE_AI_MODEL` | OpenRouter model identifier; defaults to `z-ai/glm-5.2`. |
| `OWLEYE_AI_CLICKHOUSE_URL` | Optional restricted read-only ClickHouse connection used by AI queries. |
| `OWLEYE_TRUSTED_PROXIES` | Comma-separated CIDRs for immediate reverse proxies. Leave empty when connecting directly. Forwarded addresses are ignored from other peers. |
| `OWLEYE_ALLOW_LOOPBACK_ORIGINS` | Allow SDK traffic from localhost for development. |

Setup generates `OWLEYE_HASH_SALT`, sets `SQLITE_URL` to `.owleye/owleye.sqlite`, verifies ClickHouse connectivity, builds Console and API, and creates the sole workspace and administrator. It starts Rust in the background on the selected port. Console and APIs use the same origin; no Node server runs in production. HTTP is suitable for local development; HTTPS origins automatically use Secure session cookies. The public origin must match the URL you open in your browser.

Configuration lives in `.owleye/config.json` with owner-only permissions. The initial administrator password is passed over stdin and is not stored in that file. The directory contains secrets, SQLite, logs, backups, and built releases; it is ignored by Git. Do not publish or serve it. Subsequent setup runs refuse to overwrite existing configuration. Edit configuration locally and run redeploy to apply changes.

### Running setup again

Once `.owleye/config.json` exists, a normal `./setup.sh` run exits with status 1:

```text
Already configured. Nothing was reset or restarted. Run ./redeploy.sh; setup never overwrites credentials.
```

It does not ask to reset, replace the administrator, change credentials or data, or stop/restart the existing server. The existing-configuration check runs before prerequisite commands or deployment-lock acquisition, so normal setup exits immediately even if dependencies are missing or a deployment lock already exists. Use `./redeploy.sh` to rebuild/restart or `./update.sh` to pull and redeploy. `./setup.sh --check` remains a non-installing check on an existing installation.

If the first attempt saved configuration but failed during server startup, inspect `.owleye/server.log`, correct the configuration and run redeploy. If an attempt failed before saving configuration, setup can prompt again; do not delete existing database files or configuration to force a fresh install. An administrator recovery is a separate, explicit operation described below.

If all administrator credentials are lost, run `./setup.sh --recover-admin` locally as the installation owner. It asks for an existing administrator email and a new password, revokes their sessions and authenticator setup, and never creates or promotes an account. This requires access to the private configuration and database. Sign in and reconfigure the authenticator afterward if desired.

## Redeploy

```sh
./redeploy.sh
```

Redeploy installs locked dependencies and builds both modules before touching the running process. It stops the recorded server, takes a consistent SQLite backup, starts the new build, and checks `/health/ready` plus Console delivery. Build failures leave the running server alone. Startup failures attempt to restart the previous build; schema compatibility can prevent rollback, so keep backups before updating. No automatic database restore is attempted.

Logs: `.owleye/server.log`. SQLite backups: `.owleye/backups/`. Keep a separate ClickHouse backup policy and off-machine copies of configuration and both databases. Protect the hash salt; changing it invalidates sessions and existing keys. Restrict access to backups. Manage log rotation and prune old release directories/backups after verifying newer deployments. Never delete the release recorded in `running.json` or its rollback candidate.

The scripts start a detached process, which survives the setup terminal closing. They do not install a boot service. For automatic startup after a machine reboot, use your operating system's service manager to run `./redeploy.sh` as the same dedicated, unprivileged OS user (a systemd oneshot unit with `RemainAfterExit=yes` and `KillMode=process` is one option). Give the service the same Node/pnpm/Rust PATH. Avoid running a second API process against the same port. To stop a manually started instance, verify the PID and binary in `.owleye/running.json`, then send SIGTERM and allow it to finish.

## Update

```sh
./update.sh
```

A clean Git checkout with a checked-out branch and configured upstream is required. The script uses `git pull --ff-only`, builds, backs up SQLite, and restarts. It never discards local changes or force-resets a branch. A failed build can leave source files updated while the previous deployed build continues to run; fix the build and run redeploy.

For downloaded archives, upload updated source files while preserving `.owleye/`, then run `./redeploy.sh`. `./update.sh` detects that there is no Git checkout and gives this instruction.

The administrator's Profile page checks GitHub's latest published release and links to it when a newer server version exists. No inbound webhook or public admin endpoint is needed. Checks are cached for one hour; failures show an unavailable state. Installation remains a server-side script operation. Review releases before pulling your tracked branch; the update script pulls that branch, not an arbitrary release URL.

## SDK

Use your server origin as the SDK endpoint, for example `https://analytics.example.com`, and the app's tracking ID. See the [SDK usage guide](../packages/analytics/README.md), [npm package](https://www.npmjs.com/package/@owleye/analytics), and [website](https://owleye.dev).

### Publishing the SDK

The release workflow publishes only `packages/analytics`; npm displays that folder's SDK README and includes its license. The root package is private and is not published. Author metadata identifies Shrinath, hello@shrinath.me, and https://owleye.dev. README updates appear on npm with a new package version.

Before publishing a GitHub release:

1. Commit and push the complete public source, lockfile, and `.github/workflows/release-sdk.yml` to the **public** `shrinathprabhu/owleye` repository. A release tag must point to that commit; uncommitted files are not included. The workflow must be enabled in Actions.
2. Using an npm account with administration rights for `@owleye/analytics`, open its package settings and add a GitHub Actions trusted publisher:
   - Organization/user: `shrinathprabhu`
   - Repository: `owleye`
   - Workflow filename: `release-sdk.yml` (no directory prefix)
   - Environment: `npm-publish`
   - Allowed actions: **allow direct `npm publish`**, not only staged publishing.
3. Create the matching GitHub environment `npm-publish`. Any environment tag restrictions must allow the release tag; approve the job if required reviewers are configured. This workflow needs no npm token secret.
4. Use an unused package version. The SDK release tag is `v1.0.2` (also accepted: `@owleye/analytics@1.0.2`) and must match `packages/analytics/package.json`. For subsequent releases, bump that manifest before committing and tagging. Published npm versions cannot be overwritten, including after a README edit.
5. Publish the GitHub release; a draft or a tag push alone does not trigger this workflow. It builds, tests, checks bundle sizes and package contents, then publishes from the SDK directory. Prereleases use npm's `next` tag; other releases use `latest`.
6. Check the successful Actions run, then the npm package version, SDK README, source link and provenance attestation. Confirm all three versioned CDN files become available; CDN caches can take time to update. A failed run before publishing can be rerun after fixing configuration. Check npm first if the publish outcome is uncertain; a successful version must not be republished.

The workflow uses a GitHub-hosted runner, Node 26, npm 11.5.1 or newer, `id-token: write`, and `npm publish --access public --provenance`. npm verifies the OIDC publisher against the repository, workflow and environment. Source metadata must continue to point to the public repository. The package must already exist and be managed by your npm account before configuring this trusted publisher. See [npm trusted publishing](https://docs.npmjs.com/trusted-publishers/) and [npm package READMEs](https://docs.npmjs.com/about-package-readme-files/).

### Analytics and Console storage

The [SDK privacy section](../packages/analytics/README.md#visitor-identification-and-browser-storage) describes the server-derived visitor identifiers and the SDK's lack of browser storage. Console authentication uses cookies; tab coordination uses localStorage without user IDs or credentials. Neither mechanism identifies analytics visitors. If your reverse proxy adds access logging or cookies, review that configuration separately from OwlEye. CDN delivery also makes a request to the CDN provider; serve the pinned SDK files yourself if you want to avoid that external request.

## Measurement storage upgrades

Redeploy initializes `owleye_performance` and the read-only `owleye_all_events` view in ClickHouse. The API account needs CREATE TABLE/VIEW and ALTER privileges in its own database. New Web Vitals and performance spans are stored in `owleye_performance`; historical measurements remain readable in `owleye_events`. No manual copy, reset, or cron is required. Retention and app deletion cover both tables. Back up both tables with your normal backups.

Startup also sets `non_replicated_deduplication_window = 10000` on both event tables. Transient ClickHouse insert failures get up to three retries after the initial attempt, with 1, 2, and 4-second backoff and a stable batch token. This prevents duplicate storage within the deduplication window. The queue stays bounded in memory; request and shutdown waits are capped at 180 seconds. No data reset is needed. These retries cannot recover requests that never reached the API or queued work lost in a crash; they do not bypass validation or authorization. Avoid independent bulk writes that exhaust the deduplication window during retries.

If you configured a separate read-only AI ClickHouse user, grant SELECT on `owleye_events`, `owleye_performance`, `owleye_all_events`, and `owleye_uptime_checks` in your OwlEye database after startup has created the objects and before enabling AI traffic. The view uses the caller's privileges. Never give that AI user write or schema permissions.

Self-hosted ingestion has no monthly event or subscription quota. Request-size, ingestion-rate, and queue bounds still protect the server. Performance storage uses disk space and the configured retention policy. Server integrations can create app-scoped Analytics API keys on the API Keys page; keep these secrets on trusted servers.
