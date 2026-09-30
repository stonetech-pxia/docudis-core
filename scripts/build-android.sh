#!/usr/bin/env bash
set -euo pipefail

profile="${1:-release}"
case "$profile" in
  debug|release) ;;
  *) echo "usage: $0 [debug|release]" >&2; exit 2 ;;
esac

repo_root="$(cd "$(dirname "$0")/.." && pwd)"
abis="${DOCUDIS_ANDROID_ABIS:-arm64-v8a,armeabi-v7a,x86_64}"
output="$repo_root/dist/android/$profile/jniLibs"

command -v cargo >/dev/null
command -v cargo-ndk >/dev/null || {
  echo "cargo-ndk is required: cargo install cargo-ndk --locked" >&2
  exit 1
}

args=()
IFS=',' read -r -a requested <<< "$abis"
for abi in "${requested[@]}"; do
  case "$abi" in
    arm64-v8a|armeabi-v7a|x86_64) args+=("-t" "$abi") ;;
    *) echo "unsupported Android ABI: $abi" >&2; exit 2 ;;
  esac
done

mkdir -p "$output"
build_args=(build -p docudis-capi)
if [[ "$profile" == release ]]; then build_args+=(--release); fi

(cd "$repo_root" && cargo ndk "${args[@]}" -o "$output" "${build_args[@]}")

for abi in "${requested[@]}"; do
  library="$output/$abi/libdocudis_capi.so"
  [[ -s "$library" ]] || { echo "missing $library" >&2; exit 1; }
done

"$repo_root/scripts/verify-android-artifacts.sh" "$output" "${requested[@]}"

