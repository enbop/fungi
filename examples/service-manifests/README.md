# Service Manifest Examples

This directory contains runnable examples for the Fungi `RuntimeProvider` flow.
The manifests use `spec.run` and `spec.entries` so services can participate in discovery, remote control, and local forwarding.
The examples mount `${USER_HOME}` so user-visible files live in the Fungi workspace.

Documentation:

- https://fungi.rs/docs/service-manifests

Included examples:

- `code-server.service.yaml` for the Docker-compatible container runtime path
- `filebrowser-lite-wasi.service.yaml` for the Wasmtime runtime
- `webdav-wasip2.service.yaml` for a WebDAV Wasmtime runtime service
- `run-filebrowser-lite-wasi-example.sh` to run the Wasmtime example end to end

The File Browser Lite WASI example downloads:

```bash
https://github.com/enbop/filebrowser-lite/releases/download/lite-v0.2.0/filebrowser-lite-wasi.wasm
```

The WebDAV WASI example downloads:

```bash
https://github.com/enbop/webdav-wasip2/releases/download/v0.1.0/webdav-wasip2.wasm
```

Both WASI examples expect `wasmtime serve` compatible components.

From the repository root:

```bash
cd fungi
bash examples/service-manifests/run-filebrowser-lite-wasi-example.sh
```
