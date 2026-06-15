# ADR 0002: Sequential tool-call execution

**Status**: Accepted
**Date**: 2026-06-15

## Context

agent-rig's runner no longer executes tools itself. It announces each tool
call as an `AgentEvent::ToolCall` carrying a `ToolCallRequest` with a oneshot
resolver, and arnes resolves the call against its own registry
(propose → approval → apply) before handing the outcome back through
`ToolCallRequest::resolve`. This happens in the stream loop in
`crates/arnes-core/src/session/prompt.rs`.

Within a single turn the runner issues every tool call concurrently and blocks
in a `join_all` until each request is resolved. It appends the tool-result
messages to the thread in the order the model issued them, regardless of the
order they resolve in. So the runner permits a turn's tool calls to overlap.

arnes does not take that opportunity. It resolves each `ToolCall` event inline
— running the full propose / approval / apply flow and calling `resolve`
before reading the next stream event. A turn's tool calls therefore run one
after another.

We drafted a concurrent variant (captured below) and decided against adopting
it.

## Decision

Tool calls within a turn are resolved sequentially. The stream loop runs one
tool call's propose / approval / apply flow to completion before reading the
next event.

The reasons:

- **Simplicity.** The inline loop is short and easy to follow. There is no
  `JoinSet`, no `select!` arm for completions, no cloning of session state
  behind extra `Arc`s, and no separate drain phase.
- **No same-file read-modify-write race.** `edit_text_file` and
  `write_text_file` read the file in `propose` and write it in `apply`. If two
  edits to one path overlapped, both would read the original contents and the
  second `apply` would clobber the first — a lost update. Sequential execution
  guarantees each call sees the previous call's result.
- **Predictable side effects and prompts.** Nothing runs ahead of an open
  approval prompt, and prompts appear in the order the model issued the calls.

## Alternatives considered

### Concurrent execution with a `JoinSet` (drafted, not adopted)

The drafted variant spawned each tool call's resolution onto a
`tokio::task::JoinSet` and drove the model stream and the in-flight tasks
together with `tokio::select!`, resolving each request as its task finished.
A shared `tokio::sync::Mutex` (`permission_lock`) serialized the approval
prompts so only one dialog was open at a time.

The stream loop became:

```rust
let mut tool_tasks = tokio::task::JoinSet::new();

loop {
    tokio::select! {
        maybe_ev = stream.next() => {
            let Some(ev) = maybe_ev else { break; };
            match ev.agent_event {
                // ... text / thinking / usage / cancel / error / turn-finish ...
                AgentEvent::ToolCall(req) => {
                    let registry = registry.clone();
                    let frontend = self.frontend.clone();
                    let metadata = self.tool_metadata.clone();
                    let lock = self.permission_lock.clone();
                    tool_tasks.spawn(async move {
                        let outcome = Self::resolve_tool_call_detached(
                            frontend, metadata, lock, &registry, &req,
                        )
                        .await;
                        (req, outcome)
                    });
                }
            }
        }
        Some(res) = tool_tasks.join_next(), if !tool_tasks.is_empty() => {
            if let Ok((req, outcome)) = res {
                self.frontend
                    .on_event(mk_event(EventKind::ToolCallFinished {
                        id: req.tool_call_id.clone(),
                        name: req.tool_name.clone(),
                        outcome: outcome.clone(),
                    }))
                    .await;
                req.resolve(outcome);
            }
        }
    }
}

// Drain tasks still running when the stream ends (e.g. on cancel).
while let Some(res) = tool_tasks.join_next().await {
    // ... emit ToolCallFinished and resolve ...
}
```

`resolve_tool_call_detached` was the spawn-friendly form of
`resolve_tool_call`: it took the dependencies it needed by value
(`Arc<F>` frontend, `Arc<HashMap<String, ToolKind>>` metadata,
`Arc<Mutex<()>>` permission lock) instead of borrowing `&self`, and acquired
the lock only around the `request_permission` call:

```rust
if tool.requires_approval(&req.args) {
    // ... build PermissionRequest ...
    let _permit = permission_lock.lock().await;
    if !matches!(frontend.request_permission(permission).await, Permission::AllowOnce) {
        return ToolCallOutcome::Denied;
    }
} // lock released here; propose and apply run unsynchronized
```

Adopting it required promoting `tokio` to a normal dependency of arnes-core,
storing `tool_metadata` and the new `permission_lock` behind `Arc`, and
tightening the impl bound to `F: Frontend + 'static`.

It was not adopted because the speedup is marginal for the current tools
(fast local file I/O), and the concurrency reintroduces the same-file
read-modify-write race and lets unapproved side effects run while an approval
prompt is open — costs that outweigh the benefit at this stage.

## Consequences

**Costs**

- A turn's tool calls do not overlap. Independent reads the model issues
  together run one after another even though the runner would allow them to
  overlap. This is acceptable while the tool set is fast local file I/O.

**Benefits**

- The stream loop stays small and reads in one sitting.
- No path-level locking is needed to keep concurrent edits to the same file
  correct, because edits never overlap.
- Approval prompts and tool side effects happen in the order the model issued
  the calls.

If a future tool is slow enough that parallelism matters (network calls,
sub-agents), revisit this decision. Concurrency would then need a path-level
guard for the read-modify-write tools, or a split that parallelizes only
read-only tools.
