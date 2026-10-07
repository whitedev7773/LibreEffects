#!/usr/bin/env bash
# Source your development environment first when native dependencies need it.
# Keep the same target directory and profile settings across verification runs.
set -euo pipefail
cd "$(dirname "$0")/../../.."
export CARGO_BUILD_JOBS="${CARGO_BUILD_JOBS:-1}"
export CARGO_INCREMENTAL="${CARGO_INCREMENTAL:-0}"
mode="${1:-}"
if [ "$#" -gt 0 ]; then shift; fi
case "$mode" in
  models)
    # Compiles core + editor models only, never the GPUI desktop test executable.
    exec cargo test -p libre-effects-editor-model --locked "$@"
    ;;
  check)
    exec cargo --config 'profile.dev.package.libre-effects-desktop.debug=0' \
      check --workspace --all-targets --locked "$@"
    ;;
  desktop-tests)
    # Filters select tests at runtime, not the Rust compilation unit. Keep the
    # workspace selection unchanged between focused and aggregate UI gates.
    exec cargo --config 'profile.test.package.libre-effects-desktop.debug=0' \
      --config 'profile.test.package.libre-effects-desktop.strip="debuginfo"' \
      --config 'profile.test.package.libre-effects-desktop.codegen-units=16' \
      test --workspace --locked "$@"
    ;;
  *)
    echo "Usage: bash apps/desktop/scripts/verify.sh {models|check|desktop-tests} [cargo arguments]" >&2
    exit 2
    ;;
esac
