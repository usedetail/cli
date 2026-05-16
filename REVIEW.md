# Review Guidelines

Conventions that require human judgment during review. Lint-enforced rules
(see `Cargo.toml` `[lints]` and `lib.rs` deny attributes) are omitted here.

## Conventions

- Do not use `pub(super)`. Use `pub(crate)` or `pub` instead.
- Prefer the narrowest visibility that compiles — plain `fn` over `pub(crate)`
  when the function is only used within its own module.
- Use `anyhow::Result` with `.context("…")` so the error chain shown to users
  via `Error: {err:#}` stays informative.
- `--format json` must produce valid JSON. Never mix human-readable text (hints,
  update notices) into JSON output — guard behind a format check.
- New commands that accept a `repo` positional should make it optional and fall
  back to inferring `owner/repo` from the git remote.

## Generated code

- API types come from OpenAPI codegen (progenitor). Do not hand-write types that
  duplicate generated ones. `src/api/types.rs` should only contain re-exports,
  type aliases, and trait impls on generated types.
- Update `openapi.json` via `cargo xtask generate-openapi` and help docs via
  `cargo xtask generate-help > docs/HELP.md`. Do not edit these by hand.

## Config file handling

- All writes to `config.toml` must go through `update_config(|c| …)`, which
  holds a file lock and preserves user comments and unknown keys.
- When a config field is set to `None`, remove the key from disk.

## Commit messages

- Follow conventional commits: `type(scope): description`.
- Common types: `feat`, `fix`, `refactor`, `chore`, `test`, `lint`, `docs`,
  `ci`, `style`.
