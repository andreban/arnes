# arnes-acp

An ACP (Agent Communication Protocol) server for arnes that speaks JSON-RPC 2.0 over stdio. Editors and other tools spawn it as a subprocess and exchange newline-delimited JSON messages.

**Methods accepted:** `initialize`, `session/new`, `session/prompt`, `session/cancel`.

**Notifications emitted during a prompt:** `session/update` with kinds `agent_message_chunk`, `agent_thought_chunk`, `tool_call`, and `tool_call_update` — so the client can stream the response as it arrives.

The crate exposes a `[lib]` target (`arnes_acp`) for integration testing, with the `handler`, `frontend`, `host`, and `types` modules public. The binary is `arnes-acp`. Requires `GEMINI_API_KEY` in the environment (or via `--api-key`). Logs are written to `arnes-acp.log` in the working directory.
