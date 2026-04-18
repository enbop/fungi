# fungi-stream

`fungi-stream` is a fungi-specific stream behavior built on top of libp2p's stream negotiation model.

It is intentionally narrower than a generic stream multiplexer:

- outbound streams are opened by `ConnectionId`
- inbound streams are exposed as structured events with `peer_id`, `connection_id`, `protocol`, and `stream`
- connection selection stays outside this crate, in fungi's swarm layer

## What It Solves

This crate is the single stream entrypoint for fungi.

It exists to keep three things centralized and easy to audit:

1. stream negotiation
2. per-connection stream opening
3. inbound identity-based access control

## Access Control Model

Inbound authorization uses two allow lists:

1. a global allow list for the node
2. an optional per-protocol allow list

The default listener behavior is equivalent to `*`, meaning:

- inherit the global allow list

If a protocol needs to be stricter, register it with an explicit protocol-local allow list.

Authorization is checked before an inbound stream is exposed to protocol handlers.
In practice, this happens inside the inbound upgrade path, so unauthorized peers are rejected before business logic receives the stream.

## Intended Scope

This crate is fungi-specific by design.

It is not trying to be a general-purpose replacement for `libp2p-stream`. Its goal is to provide a small, readable, testable stream core that matches fungi's security and connection-management model.