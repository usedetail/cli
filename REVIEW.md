# Review Guidelines

Rules for reviewing pull requests to this repository. Each bullet is a
convention derived from the project's commit history and enforced lint
configuration.

## Visibility

- Do not use `pub(super)`. Use `pub(crate)` or `pub` instead.
- Prefer the narrowest visibility that compiles. If a function is only used
  within its own module, plain `fn` is better than `pub(crate)`.

## Error handling

- Never use `.unwrap()`, `.expect()`, or `panic!()` in production code (all
  three are denied by clippy). In test code (`#[cfg(test)]`) they are fine.
- Use `anyhow::Result` as the return type and attach context with
  `.context("…")` so errors propagate with a human-readable chain.
- Surface user-friendly error messages. `main.rs` prints `Error: {err:#}` (the
  alternate Display chain) to stderr — keep that chain informative.
- Do not use `dbg!()`, `todo!()`, or `unimplemented!()` — all are denied.

## Type safety & casts

- No `as` casts — `clippy::as_conversions` is denied. Use `try_from` /
  `try_into`, `.saturating_*()`, or `.div_ceil()` instead.
- No `unsafe` code — `unsafe_code` is denied at the Rust lint level.
- Prefer `const fn` where possible.
- Match arms on enums must be exhaustive — `wildcard_enum_match_arm` is denied.
  Do not use `_ =>` catch-alls on enums.

## Imports & paths

- Use `use` imports at the top of the file. Do not write inline fully-qualified
  paths — `clippy::absolute_paths` is denied.
- Do not use `println!` / `eprintln!` — `clippy::print_stdout` and
  `clippy::print_stderr` are denied in `lib.rs`. Use `console::Term` for all
  terminal output.

## Generated code & OpenAPI

- API types come from OpenAPI codegen via progenitor. Do not hand-write types
  that duplicate generated ones.
- `src/api/types.rs` should contain only re-exports, type aliases, and trait
  impls (`ValueEnum`, `Formattable`) on the generated types.
- Update the vendored `openapi.json` via `cargo xtask generate-openapi`, not by
  hand. CI checks that the vendored spec matches upstream.
- Update CLI help docs via `cargo xtask generate-help > docs/HELP.md`.

## Output format discipline

- `--format json` must produce valid JSON. Never mix human-readable text (hints,
  empty-result messages, update notices) into JSON output. Guard those messages
  behind a format check.
- Auto-update notices and progress output must be suppressed when the command is
  in "silent" mode (JSON output or `completions`).
- List views use a card-based layout, not raw tables.
- Use `console::Term` for terminal output, not `println!` / `eprintln!`.

## Config file handling

- Writes to `config.toml` must go through `update_config(|c| …)`, which holds
  an exclusive file lock and preserves comments, formatting, and unknown
  user-added keys.
- Never truncate-and-rewrite the config without locking — concurrent CLI
  invocations will corrupt it.
- When a field is set to `None`, the key must be removed from disk (not left as
  an empty value).

## Concurrency

- Use file locking (`fs2`) for any shared mutable file (config, update lock).
- Auto-update uses a non-blocking try-lock so concurrent CLI invocations skip
  rather than stack up.
- Manual `detail update` uses a blocking lock so the user's explicit request
  waits instead of silently skipping.

## Testing

- Unit tests live in `#[cfg(test)] mod tests` at the bottom of each source
  file. Integration tests live in `tests/integration.rs`.
- Test edge cases: zero values, empty strings, non-ASCII input, negative
  timestamps, concurrent writes, etc.
- Do not write brittle tests that depend on exact formatting or timing.
- The `with_temp_config` helper isolates config tests via a temporary
  `XDG_CONFIG_HOME` — use it for any test that touches the config file.

## CLI argument design

- Validate inputs at the CLI boundary using clap `value_parser` ranges (e.g.
  `1..=100` for `--limit`, `1..` for `--page`).
- The `repo` positional argument should be optional when possible — the CLI
  infers `owner/repo` from the git remote `origin` when omitted.
- Use `serde(rename_all = "…")` on enums/structs rather than per-field
  `#[serde(rename)]` attributes.
- Use `serde(default)` on config structs for forward-compatibility with new
  fields.

## Code organization

- Shared helpers go in `src/utils/` (datetime, pagination, repos, git).
  Extract common logic rather than duplicating it across commands.
- Use `LazyLock` for expensive static initializations (e.g. `MadSkin`).
- Remove dead code, dead feature gates, and unused functions promptly.

## Commit messages

- Follow conventional commits: `type(scope): description`.
- Common types: `feat`, `fix`, `refactor`, `chore`, `test`, `lint`, `docs`,
  `ci`, `style`.
