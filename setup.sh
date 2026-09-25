#!/bin/sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
if ! command -v node >/dev/null 2>&1; then
  echo 'Install Node.js 26.2+, pnpm 10.28.1, Rust 1.95+ (cargo/rustc), ClickHouse and SQLite before continuing. See docs/SELF_HOSTING.md.' >&2
  echo 'Node.js: https://nodejs.org/en/download (choose Node 26, version 26.2 or newer).' >&2
  echo 'Installation guide: https://github.com/shrinathprabhu/owleye/blob/HEAD/docs/SELF_HOSTING.md#installing-prerequisites' >&2
  exit 1
fi
exec node scripts/setup.mjs "$@"
