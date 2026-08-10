# Repository Guidelines

## Project Structure & Module Organization

The Rust process plugin lives in `src/`: `main.rs` starts the dprint protocol, `handler.rs` implements formatting behavior, and `worker.rs` manages the long-lived Node worker. The TypeScript worker is in `runtime/src/worker.ts`; generated `runtime/dist/` and `runtime/dist-test/` directories are ignored. Rust unit tests are kept in `tests/unit/`, while `tests/parity.rs` exercises the built plugin end to end with files under `tests/fixtures/`. Packaging and publishing tools live in `scripts/`, and `schema/plugin.schema.json` defines the plugin manifest shape.

## Build, Test, and Development Commands

Tool versions are declared in `.prototools` (notably Deno 2.9.5, Node 22.12+, and just 1.46+). Prefer the `Justfile` entry points:

- `just install`: install locked Deno/runtime dependencies and fetch Rust crates.
- `just build`: compile the TypeScript worker and Rust executable.
- `just test`: run Node worker tests and ordinary Rust tests.
- `just e2e`: build and run the ignored process-plugin parity suite.
- `just fmt`: format TypeScript, release scripts, and Rust.
- `just check`: run formatting, linting, type checks, tests, and Clippy.
- `just ci`: reproduce the complete CI gate (`check` plus `e2e`).

## Coding Style & Naming Conventions

Rust uses edition 2024, standard `rustfmt` output (four-space indentation), `snake_case` functions/modules, and `PascalCase` types. Unsafe code is forbidden; all Clippy and pedantic warnings must pass. TypeScript is strict ESM, formatted by Oxfmt with a 100-column target and two-space indentation. Use `camelCase` for values/functions and `PascalCase` for types. Keep protocol data structured and preserve explicit error context.

## Testing Guidelines

Name Rust tests as behavior statements such as `restarts_after_transport_failure`; TypeScript tests use `*.test.ts` and Node's test runner. Add paired `*.input.*` and `*.expected.*` fixtures for formatting cases. Run `just test` during development and `just ci` before opening a pull request. No numeric coverage threshold is enforced, but new behavior and failure paths should be exercised.

## Commit & Pull Request Guidelines

History follows Conventional Commit-style subjects: `feat: ...`, `fix(runtime): ...`, `refactor(worker): ...`, and `test(integration): ...`. Use an imperative, focused subject with an optional scope. Pull requests should explain the behavior change and rationale, link relevant issues, call out packaging or compatibility effects, and report the commands run. Keep generated build output out of commits. For releases, keep `Cargo.toml`, `runtime/package.json`, and the exact `oxfmt` dependency version aligned; tags use `v<version>`.
