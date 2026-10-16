#!/bin/sh
set -eu
cd "$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)"
if ! command -v node >/dev/null 2>&1; then
  echo 'Install Node.js 26.12.0, pnpm 12.11.2, Rust 1.95+ (cargo/rustc), ClickHouse and SQLite before continuing. See docs/SELF_HOSTING.md.' >&2
  echo 'Node.js: https://nodejs.org/en/download (choose the pinned Node 26.12.0).' >&2
  echo 'Installation guide: https://github.com/shrinathprabhu/owleye/blob/HEAD/docs/SELF_HOSTING.md#installing-prerequisites' >&2
  exit 1
fi
exec node scripts/setup.mjs "$@"
