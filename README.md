# Shell

A minimal CEF (Chromium Embedded Framework) browser shell built with cef-rs.

## Building

```bash
cargo build
```

## Running

```bash
cargo run
```

## Packaging (Windows)

To create a distribution package:

```bash
cargo dist
```

This builds the release binary and packages it with all required CEF resources into `dist/Cresus/`.

## Bundling (macOS)

```bash
cargo run --bin bundle_shell
```

This creates a macOS app bundle with all required frameworks and helpers.
