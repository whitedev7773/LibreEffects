# Pinned GPUI keyboard-release boundary

Upstream: crates.io `gpui` 0.2.2, Apache-2.0; original LICENSE-APACHE retained.
Archive: https://static.crates.io/crates/gpui/gpui-0.2.2.crate
SHA-256: `979b45cfa6ec723b6f42330915a1b3769b930d02b2d505f9697f8ca602bee707`.
This is the exact previously locked dependency, not a dependency upgrade.
`UPSTREAM-SHA256.json` records original package files. Cargo's cache sentinel
and the dependency package's unused Cargo.lock are excluded; other package
files, licenses and platform resources are retained. Upstream's published
`.cargo_vcs_info.json` reports a dirty tree, so its Git SHA is not claimed to
reproduce the archive; the crates.io archive checksum is authoritative.

Local patch: `PlatformWindow::request_reliable_key_releases` defaults false;
Window exposes the delegate. X11 requests XKB DetectableAutorepeat on the
existing retained XCB connection and succeeds only when the reply reports
both supported and enabled. No new connection, raw pointers, unsafe code,
event interception or global keyboard settings are added. Audited Wayland,
macOS and Windows event adapters opt in to their existing non-synthetic-release
semantics. Other backends stay false. The app calls this once during startup
before creating modal controllers. Failures leave bounded modal activation
pointer-only with a visible explanation; native text and navigation remain.

X11's inherited same-batch release/press filter can conservatively suppress a
very fast genuine re-press (20 ms). This patch does not promise that every
release is delivered or authenticate injected events. It prevents the server
from generating autorepeat KeyUp events that can otherwise cross poll batches
and prematurely unlock a modal activation latch. No timing debounce is added.

Primary contract: https://www.x.org/archive/X11R7.7/doc/libX11/XKB/xkblib.html
(DetectableAutorepeat is optional and per-client).

This vendored source and its platform resources/manifests/build script are
included in the desktop fingerprint and Moon inputs. Renderer code is unchanged.
