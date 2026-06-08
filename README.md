# arnes

arnes is a coding-agent harness written in Rust. It pairs a single `Session` core — an LLM conversation loop with tool dispatch and usage metering — with multiple frontend adapters: a ratatui TUI for interactive use, and an ACP server for editor integration. Same harness, different faces.

## Prerequisites

- A recent stable rustc (edition 2024 requires 1.85+) — install via [rustup](https://rustup.rs/): `rustup default stable`
- A Gemini API key: `export GEMINI_API_KEY=...`

## TUI

```sh
export GEMINI_API_KEY=...
cargo install --path crates/arnes-tui
arnes
```

## ACP (editor-spawned)

```sh
export GEMINI_API_KEY=...
cargo install --path crates/arnes-acp
# editors spawn: arnes-acp
```

## Deeper reading

See [docs/build-your-own-harness/](docs/build-your-own-harness/index.html) for a full design walkthrough — from streaming client to multi-tool agent sessions.

## Building & testing

Needs a recent stable rustc (edition 2024 requires 1.85+). Install via
[rustup](https://rustup.rs/) if you don't have it: `rustup default stable`.

```sh
cargo build --workspace                                       # build all crates
cargo test  --workspace                                       # run the test suite
cargo fmt   --all -- --check                                  # format check
cargo clippy --workspace --all-targets -- -D warnings         # lint, no warnings allowed
```

CI runs the last three on Linux, macOS, and Windows on every push and PR
(see [`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

## License

Apache-2.0 — see [LICENSE](LICENSE).
