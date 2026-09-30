#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$repo_root"
cargo build -p docudis-capi

case "$(uname -s)" in
  Darwin)
    library="libdocudis_capi.dylib"
    runtime_flag="-Wl,-rpath,$repo_root/target/debug"
    ;;
  Linux)
    library="libdocudis_capi.so"
    runtime_flag="-Wl,-rpath,$repo_root/target/debug"
    ;;
  *) echo "C header smoke script supports macOS and Linux" >&2; exit 2 ;;
esac

cc crates/docudis-capi/tests/header_smoke.c \
  -Icrates/docudis-capi/include \
  -Ltarget/debug -ldocudis_capi "$runtime_flag" \
  -o target/debug/docudis_header_smoke
test -s "target/debug/$library"
target/debug/docudis_header_smoke

