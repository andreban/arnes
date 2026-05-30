# arnes

arnes is a coding-agent harness in Rust, fronted by a TUI, an ACP server, and
(later) an HTTP/WebSocket server and a GUI. One `Session` core, multiple
adapters &mdash; same harness, different faces.

## Status

Pre-alpha. The repo currently holds the design docs and a skeleton crate.
Working binaries land at the end of [M1](docs/implementation-plan/m1.html).

## Install

> Placeholders &mdash; the crates referenced below land in M1 (workspace split
> happens in PR #1) and `arnes-gui` / `arnes-serve` are M8&ndash;M9.

```sh
cargo install --path crates/arnes-tui    # `arnes`     &mdash; ratatui TUI (M1)
cargo install --path crates/arnes-acp    # `arnes-acp` &mdash; ACP server on stdio (M1)
# cargo install --path crates/arnes-serve  # `arnes-serve` &mdash; HTTP/WS (M8)
# cargo install --path crates/arnes-gui    # `arnes-gui`   &mdash; desktop GUI (M9)
```

## Docs

The design lives in-tree and is meant to be read in a browser.

- [Implementation plan](docs/implementation-plan/index.html) &mdash; architecture, roadmap, and per-milestone work plans.
- [Build Your Own Harness](docs/build-your-own-harness/index.html) &mdash; the longer-form book that motivates the design.

## License

Apache-2.0 &mdash; see [LICENSE](LICENSE).
