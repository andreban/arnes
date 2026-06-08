# ADR 0001: JSON-RPC library for ACP transport

**Status**: Accepted
**Date**: 2026-06-08

## Context

The ACP transport layer requires bidirectional JSON-RPC 2.0 over a single stdio channel.
The server must:

- handle inbound requests from the client (`initialize`, `session/new`,
  `session/prompt`, `session/cancel`)
- send server-initiated outbound requests back to the client
  (`fs/read_text_file`)
- send server-initiated notifications (`session/update`)

All of this happens over the same pipe — there is no separate channel for
each direction. The inbound read loop must remain live while a long-running
`session/prompt` is in flight so that cancellation requests and outbound
responses can be received concurrently.

The M1 plan (Phase 5) designated this as a library-decision spike.
`jsonrpsee` was the initially preferred option; a hand-rolled framer was
the explicit fallback.

## Decision

We use the hand-rolled framer approach. There is no `jsonrpsee` dependency
in any crate.

The transport is implemented as:

- **Framing**: `tokio_util::codec::FramedRead` with `LinesCodec::new()` —
  one UTF-8 JSON object per newline-delimited line.
- **Types**: custom `Request`, `Response`, `Notification`, and
  `OutboundRequest` structs in
  `crates/arnes-acp/src/types/jsonrpc.rs` (~90 lines total).
- **Dispatch**: manual `match req.method.as_str()` block in `main.rs`.
  `session/prompt` is spawned onto a new task so the read loop stays live.
- **Outbound request correlation**: `PendingRequests` — an
  `Arc<Mutex<HashMap<String, oneshot::Sender<Result<Value, String>>>>>`,
  keyed by UUID v7 request IDs with a 30-second timeout.

## Alternatives considered

### `jsonrpsee` (original plan, not adopted)

`jsonrpsee` is a full async JSON-RPC framework. The M1 plan proposed
starting with it and falling back to hand-rolled only if a trigger fired.
When scoping the implementation, it became clear that `jsonrpsee`'s
server-side transport abstractions are designed primarily for TCP and
WebSocket; adapting them to a shared stdin/stdout channel with
server-initiated calls is not a documented first-class use case. Given
that a complete, correct hand-rolled framer fit in under 200 lines with
no new dependencies, the library overhead was not justified.

### `tower-lsp` internals

Rejected. `tower-lsp` wraps a JSON-RPC dispatcher but all types are
coupled to Language Server Protocol shapes. Extracting the transport
would require fighting LSP-specific assumptions throughout.

## Fallback trigger analysis

The M1 plan listed three explicit triggers for switching from `jsonrpsee`
to hand-rolled. Each is evaluated against the actual implementation.

### Trigger 1: outbound-call helpers need >10 lines of boilerplate per method

**Did not fire.** Each outbound call uses `OutboundRequest::new(id,
method, params)` plus serialisation and an mpsc send — 3–5 lines per
call site, well under the threshold. Adding a second outbound method in a
future milestone will cost roughly the same.

### Trigger 2: request-correlation machinery interferes with per-session `CancellationToken` wiring

**Did not fire.** The `PendingRequests` map and the per-session
`CancellationToken` are entirely independent data structures. Cancelling a
session prompt cancels the `Session::prompt` future; pending outbound
requests are cleaned up independently on timeout or channel close. The two
mechanisms do not interact.

### Trigger 3: adding ACP `_meta` field requires fighting the serialization layer

**Did not fire (not yet needed).** ACP's `_meta` envelope field is unused
in M1. When it is needed (M7 compaction, per the M1 plan), adding it to
the hand-rolled structs is a plain field addition with
`#[serde(skip_serializing_if = "Option::is_none")]`. No friction is
anticipated.

## Consequences

**Costs**

- No library-provided protocol validation; malformed frames are caught at
  parse time and return a single `-32700`/`-32600` error response.
- Each new JSON-RPC method requires a dispatch arm in `main.rs` and
  corresponding parameter/result types.
- No automatic parameter schema validation — each handler calls
  `serde_json::from_value` and returns `-32602` on failure.

**Benefits**

- Zero external library surface. Outbound calls, notification sends, and
  response routing are plain Rust with no proc-macro indirection.
- `_meta` and other ACP-specific envelope fields are trivial to add to
  the hand-rolled structs.
- The full transport implementation is small enough to read in one sitting;
  there are no `jsonrpsee` internals to debug when something goes wrong.
- `tokio_util` is already in the workspace dependency tree (used for
  `CancellationToken`); `LinesCodec` required no new crates.
