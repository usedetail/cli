# Agent Guidance

## Core Commands

```bash
# Build
cargo build

# Format check
cargo fmt --check

# Lint
cargo clippy -- -D warnings

# Type check
cargo check

# Run tests
cargo test

# Check vendored artifacts (openapi.json + docs/HELP.md) are up to date
cargo xtask check

# Regenerate docs/HELP.md from current CLI definitions
cargo xtask generate-help > docs/HELP.md

# Regenerate openapi.json from upstream API
cargo xtask generate-openapi
```

## CI

CI runs formatting, clippy, check, tests, vendored artifact checks, and a security audit. All must pass before merge.

## Lint Policy

Clippy is configured with strict `pedantic`, `nursery`, and many restriction lints denied. See `[lints.clippy]` in `Cargo.toml` for the full list. Notable: `unwrap_used`, `expect_used`, `panic`, and `unsafe_code` are all denied outside of tests.
