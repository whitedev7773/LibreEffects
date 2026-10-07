# Repository Guidelines

## Project Structure

This repository contains Libre Effects, a motion graphics editor derived from the OpenCut rewrite, managed with Moon and proto.

- `apps/web/`: React and TanStack Start frontend, with Tailwind CSS and Cloudflare deployment. File-based routes live in `src/routes/`, reusable UI primitives in `src/components/ui/`, hooks in `src/hooks/`, and static assets in `public/`.
- `apps/api/`: Elysia API running as a Cloudflare Worker. Its entry point is `src/index.ts`; preserve the Cloudflare adapter and required `.compile()` call.
- `apps/desktop/`: Rust/GPUI application. Shared UI components live in `src/components/`, editor panels in `src/panels/`, and shell/theme code in `src/shell.rs` and `src/theme.rs`.
- `crates/`: shared core work and media setup tooling. `apps/desktop` and `crates/core` are registered in the root Cargo workspace; add new Rust crates to its members explicitly.
- `brand/marks/`: SVG brand assets. `changelog/`: release notes. `.moon/`: workspace and toolchain configuration.

Do not manually edit `apps/web/src/routeTree.gen.ts`; TanStack Router generates it.

## Setup and Development

Run `proto use` from the repository root to install the versions pinned in `.prototools`. Use Bun for JavaScript dependencies and Moon for project tasks. Consult each app's `package.json` and `moon.yml` before adding commands.

| Command | Purpose |
| --- | --- |
| `moon run web:dev` | Start the frontend at `http://localhost:5173` |
| `moon run api:dev` | Start the local Worker at `http://localhost:8787` |
| `moon run desktop:dev` | Compile and launch the GPUI desktop app |
| `moon run web:build` | Build the frontend |
| `moon run api:build` | Validate Worker packaging using Wrangler's deployment dry run |
| `moon run desktop:check` | Type-check the desktop crate |
| `moon run desktop:build` | Build the desktop release binary |

Desktop platform prerequisites are documented in `apps/desktop/README.md`. The initial GPUI build can take a while.

## Coding Conventions

TypeScript is strict: avoid unused symbols, use explicit type imports, and prefer `#/*` or `@/*` aliases for web source imports. Default to two-space indentation, double quotes, and semicolon-free TS/TSX. Existing formatting varies; keep edits focused and avoid unrelated reformatting. Use PascalCase React component names, `use-*` hook filenames, and kebab-case UI filenames.

Follow the applicable quality and accessibility rules in `.github/copilot-instructions.md`: use semantic controls, associated labels, accessible names, and keyboard interaction. That file mentions Ultracite/Biome, but the app manifests currently define no lint or format scripts.

Use `rustfmt` for Rust, with `snake_case` modules/functions and `PascalCase` types.

## Validation

- Before frontend submissions, run `moon run web:test` and, for build-affecting changes, `moon run web:build`. Vitest and Testing Library are installed; focused utility regressions use `vitest.config.ts`. Add behavior/regression tests beside source as `*.test.ts` or `*.test.tsx`; configure a DOM environment when needed. Report an empty test suite accurately rather than claiming tests passed.
- For API changes, run `moon run api:build`.
- For Rust changes, run `moon run desktop:check`, `cargo fmt --all --check`, and `cargo test --workspace`. Add unit tests in `#[cfg(test)]` modules or integration tests under `apps/desktop/tests/` as appropriate.
- Documentation-only changes need review and `git diff --check`, not application builds.

## Commits and Pull Requests

Use short, imperative, scoped subjects such as `web: add timeline zoom controls`. Keep commits focused. PR descriptions should explain behavior changes and validation, link relevant issues, include screenshots for visible UI changes, and note configuration or deployment impacts.

The inherited `.github/pull_request_template.md` describes contributions to upstream OpenCut. Work in this fork follows the owner's requested scope; do not publish to upstream, merge or deploy without authorization.
