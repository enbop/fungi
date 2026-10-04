# Authoring `.fungi.md` service files

A service file is Markdown with strict YAML frontmatter. Unknown fields are rejected. Use `fungi service apply NAME FILE --dry-run` locally or `fungi service apply NAME@DEVICE FILE --dry-run` remotely as the authoritative validator.

## Common shape

```yaml
---
fungi: service/v1
id: example-service

run:
  provider: wasmtime
  source:
    file: ./app.wasm
  env:
    EXAMPLE_MODE: production
  mounts:
    - from: $fungi.service.data
      to: /data

publish:
  web:
    tcp:
      port: 8080
    client:
      kind: web
      path: /
---

# Example service

Explain what runs, what data is mounted, what is reachable, and relevant safety assumptions.
```

This is a structural example, not a downloadable application. Replace `./app.wasm` with a verified WASI command component that owns a TCP listener on the published port, and supply only the arguments and environment that component actually supports.

`id` identifies the reusable definition. `service apply NAME ...` sets the deployed instance name, so one definition can back differently named instances.

## Choose one runtime pattern

The current managed provider is Wasmtime. It requires exactly one of `run.source.url` or `run.source.file`; a relative file is resolved from the service file's directory. Fungi runs a WASI command component with `wasmtime run`, and the component must provide its own listener for published TCP endpoints. An HTTP component designed only for `wasmtime serve` needs a compatible command-component build; removing its mode field does not convert it.

```yaml
run:
  provider: wasmtime
  source:
    url: https://example.invalid/releases/v1/app.wasm
```

The URL above is a placeholder; use a verified, preferably pinned release artifact. The current schema rejects `run.mode`, `provider: docker`, and `run.source.image`. If an older installed CLI exposes different providers or modes, use its current help and a version-compatible official recipe rather than applying this schema blindly.

To publish an already-running local TCP service, omit `run`. Exactly one publish entry is allowed, and the host must be `127.0.0.1` or `localhost`:

```yaml
publish:
  ssh:
    tcp:
      host: 127.0.0.1
      port: 22
    client:
      kind: ssh
```

## Field rules

- `fungi` must equal `service/v1`; `id` must be non-empty; `publish` needs at least one entry.
- `run` accepts `provider: wasmtime`, `source`, `args`, `env`, and `mounts`.
- Each mount contains `from` and runtime-side `to`.
- Prefer `$fungi.service.data` for private persistent app data and `$fungi.service.artifacts` for service artifacts. `$fungi.workspace` exposes the user's Fungi workspace; `$fungi.root` is broader and requires special care.
- Each publish entry contains `tcp.port` greater than zero and optional `client` metadata.
- `client.kind: web` may use `path` and `iconUrl`; `ssh` expresses SSH intent; other kinds are treated as raw clients.
- All publish entries currently need matching client metadata.
- Wasmtime and existing-TCP ports are fixed local ports and must not collide. Publish hosts must be loopback (`127.0.0.1` or `localhost`); omission defaults to `127.0.0.1`.
- Keep credentials out of frontmatter. Confirm how the workload receives secrets before deploying it.

## Validation loop

```bash
fungi info runtime
fungi service apply NAME@DEVICE ./NAME.fungi.md --dry-run
```

Inspect an existing service, then choose the apply/start sequence from the [service lifecycle table](cli-workflows.md#choose-the-final-service-state). After applying, verify:

```bash
fungi service inspect NAME@DEVICE --verbose
fungi service logs NAME@DEVICE --tail 200
```

Omit `@DEVICE` for a local service.

If validation fails, change only the reported field or runtime assumption, then rerun `--dry-run`. If apply or startup fails, preserve the file and reconcile the manifest, observed state, and bounded logs before choosing a correction or retry. When the service should be running, also verify its published endpoint from the authorized controller as described in the [result verification workflow](cli-workflows.md#verify-the-result-before-retrying).
