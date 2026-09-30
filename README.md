# Libre Effects

A Windows-first, open source motion graphics and compositing editor, forked from
[OpenCut](https://github.com/opencut-app/opencut). The goal is a composition and
layer workflow familiar to After Effects users, with extensive scripting support.

[![License: MIT](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

## Status

**Early desktop foundation.** Rectangle layers, transform properties, keyframes,
interpolation, preview/playback, undo/redo, and JSON project persistence are implemented.
The shared Rust editing model lives in `crates/core`.

JSX/ExtendScript compatibility, expressions, advanced motion graphics tools, media
import, and export remain future work. Web and API apps are inherited from the
OpenCut rewrite and do not expose the desktop editor yet.

See [the desktop guide](apps/desktop/README.md) for the animation walkthrough and
current limitations. Original OpenCut copyright and MIT license notices are retained.

## Development

Install [proto](https://moonrepo.dev/proto) if you haven't already:

**Linux, macOS, WSL:**

```sh
bash <(curl -fsSL https://moonrepo.dev/install/proto.sh)
```

**Windows (PowerShell):**

```powershell
irm https://moonrepo.dev/install/proto.ps1 | iex
```

If shims fail to run, allow local scripts for your user:

```powershell
Set-ExecutionPolicy -Scope CurrentUser RemoteSigned
```

From the repo root:

```sh
proto use    # installs the tools pinned in .prototools
```

```sh
moon run web:dev       # localhost:5173
moon run api:dev       # localhost:8787
moon run desktop:dev   # see apps/desktop/README.md
```

## Contributing

We're not set up to take outside contributions yet while the architecture is being designed. If you want to follow along, ask questions, or just hang out, [join the Discord](https://discord.gg/zmR9N35cjK) or [open an issue](https://github.com/opencut-app/opencut/issues).

## Sponsors

OpenCut is supported by companies that believe in open source creator tools.

- [**fal.ai**](https://fal.ai?utm_source=github-opencut&utm_campaign=oss): Generative image, video, and audio models all in one place.

Want your logo here? Reach out at [sponsor@opencut.app](mailto:sponsor@opencut.app).

## License

[MIT](LICENSE)
