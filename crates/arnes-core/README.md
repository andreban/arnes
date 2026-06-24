# arnes-core

The shared library at the heart of the arnes harness. It provides the `Session<F>` type — a generic conversation loop that drives an LLM through multi-turn exchanges, dispatches tool calls, and accumulates usage metrics — along with the traits that frontend adapters and environment hosts must implement.

## Public surface

- **`Session<F>`** — create with `Session::new(frontend, host, model, model_key)`, then call `session.prompt(text, cancel).await` to run one turn.
- **`Frontend` + `SessionEvent`** — the trait an adapter implements to receive streaming events (text chunks, thinking blocks, tool lifecycle notifications, turn end/stop).
- **`Host`** — the trait that wires the agent to the local environment; compose it from `ReadTextFile`, `WriteTextFile`, and `Terminal` implementations.
- **`Message` / `ContentBlock`** — conversation message types.
- **`CumulativeUsage` / `TurnUsage` / `TokenCounts`** — token metering accumulated across turns.
- **`ToolContext`** — per-invocation context passed into tool implementations, bundling the `Host`, working directory, and read grants.
- **`CoreError` / `Result`** — the crate's error type.
