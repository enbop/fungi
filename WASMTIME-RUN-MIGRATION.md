# Upgrading Wasmtime services to run

Fungi now launches Wasmtime services only through `fungi run`. The `fungi serve`
command and `run.mode` manifest field have been removed. Services must export
`wasi:cli/run` and own their TCP listener, including its address and port. Outgoing
`wasi:http` requests, including host HTTPS, remain available through `run`.

## Existing installations

Core 0.7.1 persisted HTTP services with `run.mode: http` in
`services/<local_service_id>/service.yaml`. After upgrading, these services appear
with `status.phase: unknown` and a `configuration error` in `status.detail`.
They do not start automatically or prevent the daemon and healthy services from
starting. `service inspect` explains the error; `service start` reports the same
reason. Local and remote service management use the same status path.

Use the original instance name when applying the upgraded recipe:

```sh
fungi service inspect my-files
fungi service apply my-files --recipe filebrowser-lite --refresh --start
```

Run this after the run-compatible recipe and component release are published.
For a local updated service file, use:

```sh
fungi service apply my-files ./updated.fungi.md --start
```

A successful apply rewrites `service.yaml` and `state.json`, clears the error,
restages the component, and reuses the local service ID and
`appdata/services/<local_service_id>`. Failed configurations are upgraded into a
stopped state unless `--start` is requested. An existing known recipe definition
ID must still match. Removing only `run.mode` does not convert an
incoming-handler-only component into a command component.

For pre-0.7 `services-state.json` installations, the normal backed-up directory
migration preserves the old HTTP service's instance name and data, and records a
`configuration_error` in its `state.json`. The old format did not have a recipe
definition ID; that identity remains unspecified until the user applies an
updated recipe. The migrated HTTP manifest is intentionally not executable.

## Other configuration errors

Malformed or unsupported service manifests and service state files use the same
per-service error path. The daemon leaves their files unchanged during loading.
If the instance name cannot be recovered, the entry is shown under its local
service ID. Duplicate names are also isolated and addressed by local service ID
so applying a recipe cannot silently pick the wrong data directory.

Correct saved files and restart the daemon, apply a supported definition, or
remove the entry with `fungi service remove <name>`. Removing a service preserves
its appdata. For an unreadable configuration, removal clears the saved service
entry without guessing how to operate a runtime from invalid configuration.
Directory-wide I/O failures and invalid daemon configuration still require
repair before startup.

## Release order

1. Merge and release the run versions of filebrowser-lite and webdav-wasi.
2. Publish the official recipes pointing at those released artifacts.
3. Release this core change; align the app's bundled core version when updating it.

Updating the recipe catalog alone does not rewrite an installed service. Users
of the old HTTP components must apply the upgraded recipe explicitly.
