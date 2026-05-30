# AGENTS.md

Rules for any coding agent &mdash; human or LLM, including arnes itself once
it can drive its own repo &mdash; working on this codebase. Keep these
internally consistent with what CI enforces; if you change one, change both.

## Rust style

- Format with `rustfmt`. Run `cargo fmt --all` before committing; CI runs
  `cargo fmt --all -- --check`.
- Lint clean under `cargo clippy --workspace --all-targets -- -D warnings`.
  No allow-attributes to silence clippy unless you also leave a one-line
  comment explaining why.
- Public items in `arnes-core` get doc comments. `cargo doc` is expected to
  build with warnings-as-errors (`RUSTDOCFLAGS="-D warnings"`).
- Prefer `thiserror` for library error types; `anyhow` is for binaries and
  tests.

## MSRV

Pinned in `[workspace.package].rust-version` once the workspace lands in
PR #1. Policy: latest stable minus two minor versions, bumped deliberately
in its own PR (never piggy-backed onto a feature change). Edition is `2024`.

## Commit messages

- Imperative subject, &le;72 characters, no trailing period.
- Body wrapped at 72 columns, separated from the subject by a blank line,
  explaining the *why* (the *what* is in the diff).
- Reference the relevant PR slug from `docs/implementation-plan/m1-plan.html`
  (e.g. `PR #2`) when a commit corresponds to one.
- Do **not** include `Co-Authored-By:` trailers or "Generated with" footers.

## License headers

Every source file starts with a two-line header:

```rust
// Copyright <year> Andre Cipriani Bandarra
// SPDX-License-Identifier: Apache-2.0
```

The year is the file's creation year &mdash; do not bump it on later edits.
Per-file SPDX identifiers let tools like REUSE and GitHub's license
detector resolve the license unambiguously even when individual files are
copied out. The top-level [`LICENSE`](LICENSE) is the source of truth for
the full text.

This applies to `.rs` files (and any future code files). It does not
apply to `Cargo.toml`, Markdown, or the rendered HTML docs.

## Never commit

- `target/`, `Cargo.lock` for purely-library crates (the workspace ships
  binaries, so the root `Cargo.lock` *is* committed).
- Editor / OS detritus: `.vscode/`, `.idea/`, `.DS_Store`, `*.swp`.
- Secrets: `.env`, anything with an API key. `GEMINI_API_KEY` is read from
  the environment at runtime; never check a fallback in.

## Tests

- Unit tests live next to the code they test, in `#[cfg(test)] mod tests`.
- Integration tests live under each crate's `tests/` directory. Shared test
  helpers under `tests/support/` (per PR #4 of the M1 plan).
- Run the full suite with `cargo test --workspace`. CI runs this on Linux,
  macOS, and Windows.
- Tests that need a real provider key are `#[ignore]` by default and run
  with `cargo test -- --ignored` in a CI job that has the secret available.

## Before opening a PR

```sh
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

All three must pass. If they do not, fix the underlying issue &mdash; do not
disable hooks or skip lints to get green.
