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
