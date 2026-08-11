# dprint-process-plugin-oxfmt

`dprint-process-plugin-oxfmt` is a dprint process plugin backed by the official asynchronous
`oxfmt.format()` JavaScript API. It keeps one Node worker alive per dprint session and targets
byte-for-byte single-file Oxfmt output.

## Installation

Install the plugin as a development dependency:

```sh
npm install --save-dev dprint-process-plugin-oxfmt@0.63.0
```

Reference the package manifest in `dprint.json`:

```json
{
  "plugins": ["npm:dprint-process-plugin-oxfmt/plugin.json"],
  "oxfmt": {
    "printWidth": 100,
    "singleQuote": true,
    "semi": true
  }
}
```

Node `>=22.12.0` is required and is not bundled. The main npm package selects the appropriate
optional platform package. dprint verifies the selected platform tarball against the checksum in
`plugin.json`.

For an explicitly pinned main tarball, use:

```text
npm:dprint-process-plugin-oxfmt@0.63.0/plugin.json@<main-package-tarball-sha256>
```

## Development

The repository uses proto. `.prototools` pins Deno `2.9.5` and declares the Node, just, and dprint
requirements. Deno manages the release tools and Runtime npm dependencies through `deno.json` and
the shared `deno.lock`.

```text
just install          # Install locked Deno/Runtime dependencies and fetch Rust dependencies
just build            # Build the Node worker and Rust executable
just fmt              # Format Runtime, release tools, and Rust
just test             # Run Runtime and ordinary Rust tests
just e2e              # Run the process-plugin parity suite
just check            # Run format, lint, type, test, and Clippy checks
just ci               # Run check and e2e
```

The release version must match in `Cargo.toml`, `runtime/package.json`, and the exact `oxfmt`
dependency. A release tag must be `v<version>`.

## npm packaging

The distribution is npm-only. There are no native outer tarballs, checksum sidecars, single-platform
manifests, or GitHub Release assets.

```text
just package          # Build this host and write one official platform ZIP
just npm-package      # Discover all platform ZIPs and generate npm packages
just npm-publish      # Preflight every package, then publish main last
```

`just package` asks official dprint automation for the current platform and standard ZIP name. Its
only output is:

```text
dist/npm-inputs/<official-platform>/<official-standard-name>.zip
```

The ZIP contains the Rust executable and its sibling production Runtime, including its Oxfmt native
binding. `just npm-package` dynamically discovers and extracts these directories, then calls official
`createDprintOrgNpmPackages()` with each complete payload as `packageContents`.
Official automation owns package suffixes, npm `os`/`cpu`/`libc`, optional dependencies, references,
and platform tarball checksums.

Release CI builds ZIPs on five native runners. The aggregate job downloads all successful ZIPs,
discovers platforms dynamically, and generates the complete npm release. Tag publishing downloads
only the final `.tgz` files, reads package identities and versions from them, performs all
repository/version preflights, publishes platform packages in package name order, and publishes
`dprint-process-plugin-oxfmt` last through npm Trusted Publisher.

## Security

This process plugin is not sandboxed. Formatting may execute trusted project Tailwind configuration,
plugins, and other JavaScript-backed formatter configuration. Use it only in repositories you trust.
