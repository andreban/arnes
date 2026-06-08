# arnes-tui

A full-screen terminal chat interface for arnes, built with [ratatui](https://ratatui.rs/) and [crossterm](https://github.com/crossterm-rs/crossterm). Run it as `arnes` after installing.

The TUI streams LLM responses in real time, renders thinking blocks in a distinct style, and surfaces tool calls as they execute. It wires arnes-core's `Session` to a ratatui frontend and a local-filesystem host, so you get the full coding-agent loop in a single terminal window.

Requires `GEMINI_API_KEY` in the environment (or via `--api-key`). Logs are written to `arnes-tui.log` in the working directory.
