# Requires just 1.46.0.

set shell := ["bash", "-eu", "-o", "pipefail", "-c"]

install:
    cargo fetch
    deno install --frozen

build:
    deno task runtime:build
    cargo build

fmt:
    deno task fmt
    cargo fmt --all

test: install
    deno task runtime:test
    cargo test

e2e: build
    cargo test --test parity -- --ignored --nocapture

check: install
    deno task fmt:check
    cargo fmt --all -- --check
    deno lint
    deno check --frozen scripts/release/*.ts scripts/npm/*.ts runtime/src/*.ts
    deno task runtime:test
    cargo test
    cargo clippy --all-targets -- -D warnings

ci: check e2e

package:
    deno run -A --frozen scripts/release/package.ts

npm-package:
    deno run -A --frozen scripts/npm/package.ts

npm-publish:
    deno run -A --frozen scripts/npm/publish.ts
