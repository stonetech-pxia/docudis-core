#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 2 ]]; then
  echo "usage: $0 JNI_LIBS_DIR ABI [ABI ...]" >&2
  exit 2
fi

directory="$1"
shift

nm_tool="${ANDROID_NM:-}"
if [[ -z "$nm_tool" && -n "${ANDROID_NDK_HOME:-}" ]]; then
  nm_tool="$(find "$ANDROID_NDK_HOME/toolchains/llvm/prebuilt" -type f -name llvm-nm -print -quit)"
fi
if [[ -z "$nm_tool" ]]; then nm_tool="$(command -v llvm-nm || true)"; fi
[[ -n "$nm_tool" ]] || { echo "set ANDROID_NM or ANDROID_NDK_HOME" >&2; exit 1; }

required=(
  docudis_v1_abi_version
  docudis_v1_anonymize_json
  docudis_v1_detect_json
  docudis_v1_process_json
  docudis_v1_restore_json
  docudis_v1_buffer_free
  docudis_v1_last_error_message
)

for abi in "$@"; do
  library="$directory/$abi/libdocudis_capi.so"
  [[ -s "$library" ]] || { echo "missing $library" >&2; exit 1; }
  symbols="$($nm_tool -D --defined-only "$library")"
  for symbol in "${required[@]}"; do
    grep -q " $symbol$" <<< "$symbols" || {
      echo "$library does not export $symbol" >&2
      exit 1
    }
  done
done

