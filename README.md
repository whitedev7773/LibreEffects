# Libre Effects

A Windows-first, open source motion graphics and compositing editor, forked from
[OpenCut](https://github.com/opencut-app/opencut). The goal is a composition and
layer workflow familiar to After Effects users, with extensive scripting support.

[![License: MIT](https://img.shields.io/badge/license-MIT-green?style=flat)](LICENSE)

## Status

**Early 2D desktop editor.** The Rust/GPUI application supports multiple
compositions, layers and precompositions, shared media import, text and vector
Contents, masks, effects, scalar Value/Speed Graph editing, project recovery,
render queues, and PNG/H.264/ProRes output. Preview and export share the compositor.
The shared editing model lives in `crates/core`.

This is not a complete After Effects replacement. JSX/ExtendScript, expressions,
3D, tracking, Adobe project compatibility and the web editor remain separate
future work. Audio-device preview currently targets Windows; file rendering and
other editor workflows are also validated on Linux.

See [the desktop guide](apps/desktop/README.md) for current behavior,
[the current implementation status](apps/desktop/STATUS.md) for verified work and
remaining milestones, and [the development backlog](apps/desktop/DEVELOPMENT_BACKLOG.md)
for the dated design and validation history. Original OpenCut copyright and MIT
license notices are retained.

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
