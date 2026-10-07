#!/usr/bin/env bash
# GPUI-free tests of the exact production XKB reply-mask interpreter.
set -euo pipefail
root="$(cd "$(dirname "$0")/../../.." && pwd)"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
cp "$root/vendor/gpui/src/platform/linux/x11/repeat_capability.rs" "$tmp/repeat_capability.rs"
printf 'mod repeat_capability;\n' > "$tmp/harness.rs"
rustc --test --edition 2024 "$tmp/harness.rs" -o "$tmp/reply-tests"
"$tmp/reply-tests"
