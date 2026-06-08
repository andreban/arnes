# Hello, arnes

A quick walkthrough to get from zero to a working agent session in under ten minutes.

## 1. Clone and build

```sh
git clone https://github.com/andreban/arnes
cd arnes
cargo build --workspace
```

You need a recent stable rustc (1.85+ for edition 2024). If you don't have it:

```sh
rustup default stable
```

## 2. Get a Gemini API key

arnes uses Gemini as its LLM backend. Obtain an API key from [Google AI Studio](https://aistudio.google.com/) and export it:

```sh
export GEMINI_API_KEY=your_key_here
```

## 3. First TUI conversation

Install and launch the TUI:

```sh
cargo install --path crates/arnes-tui
arnes
```

Type a message and press **Enter**. The agent streams its reply in real time, with thinking blocks shown in a distinct style above the response. Press **Ctrl-C** to exit.

## 4. First ACP exchange via stdin

Install the ACP server:

```sh
cargo install --path crates/arnes-acp
```

Then drive it manually with newline-delimited JSON-RPC 2.0. The session below uses `arnes-acp` directly on the command line (pipe input via `echo` or a file):

```sh
# Step 1 — initialize
echo '{"jsonrpc":"2.0","id":"1","method":"initialize","params":{}}' | arnes-acp
```

In practice, editors spawn `arnes-acp` as a long-lived subprocess and exchange messages over its stdin/stdout. See `crates/arnes-acp/tests/golden/m1_basic.jsonl` for a recorded initialize → session/new → session/prompt exchange.

## 5. Going further

- [Build Your Own Harness](build-your-own-harness/index.html) — chapter-by-chapter walkthrough of the arnes design, from a bare streaming client to a full multi-tool session.
- `crates/arnes-core/README.md` — the public API you implement against when writing a new frontend adapter.
