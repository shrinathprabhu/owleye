#!/bin/sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
if ! command -v node >/dev/null 2>&1; then
  echo 'Install Node.js 26.2+, pnpm 10.28.1, Rust 1.95+ (cargo/rustc), ClickHouse and SQLite before continuing. See docs/SELF_HOSTING.md.' >&2
  exit 1
fi
exec node scripts/redeploy.mjs "$@"
