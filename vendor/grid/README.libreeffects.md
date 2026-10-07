# LibreEffects grid security backport

This is the published `grid` 0.18.0 crate (MIT), with the checked-dimension
arithmetic from upstream commit
[`be213bd3528727148bef2d523c89e95d1fd9c072`](https://github.com/becheran/grid/commit/be213bd3528727148bef2d523c89e95d1fd9c072)
backported to `expand_rows` and `expand_cols`. Version 0.18 has no prepend
methods, so those upstream hunks do not apply.

The fix addresses
[GHSA-38c5-483c-4qqp / CVE-2026-42199](https://github.com/becheran/grid/security/advisories/GHSA-38c5-483c-4qqp).
Dimension addition and multiplication are checked before storage or logical
dimensions change. The existing 0.18 public API and feature names are preserved.

## Why a backport?

GPUI 0.2.2 pins Taffy to exactly 0.9.0; that Taffy version requires `grid ^0.18`.
Upstream's first patched release, grid 1.0.1, does not satisfy that requirement.
The root Cargo patch uses this source instead of forcing a semver-incompatible
version or upgrading the application's GUI framework wholesale.

`+libreeffects.1` identifies this local build. This is a source-level backport,
not an upstream 0.18 security release. Version-only advisory tools may still flag
it. Do not suppress the advisory globally. Remove the patch and this directory
when a tested upstream GPUI/Taffy combination resolves grid >=1.0.1.

## Provenance

- crates.io package: grid 0.18.0
- Original package SHA-256:
  `12101ecc8225ea6d675bc70263074eab6169079621c2186fe0c66590b2df9681`
- Original repository revision: `1d64a14ad59f2bdf1ab944a267532a1f2a780815`
- `LICENSE`, upstream README, and the complete source/unit tests are retained.
- Upstream CRLF source is normalized to LF and trailing comment whitespace is
  trimmed. Unused benchmark dependencies and
  packaging metadata are omitted from the local manifest; the serde feature
  and its existing tests remain available.

## Validation

Run the upstream suite and the local safety checks in release mode as well as
debug mode, so overflow-check settings cannot hide a regression:

```sh
cargo test --manifest-path vendor/grid/Cargo.toml --locked --all-features
cargo test --manifest-path vendor/grid/Cargo.toml --locked --all-features --release
cargo check --manifest-path vendor/grid/Cargo.toml --locked --no-default-features
cargo check -p libre-effects-desktop --locked
cargo test --workspace --locked
```
