# Service Manifest Examples

This directory contains runnable examples for the Fungi `RuntimeProvider` flow.
The manifests use `spec.expose` so services can participate in discovery, remote control, and local forwarding.
The examples use `${APP_HOME}` for per-service storage and `hostPort: auto` so the target node resolves paths and ports locally.

Documentation:

- https://fungi.rs/docs/service-manifests

Included examples:

- `filebrowser.service.yaml` for the Docker-compatible container runtime path
- `filebrowser-lite-wasi.service.yaml` for the Wasmtime runtime
- `run-filebrowser-example.sh` to run the container example end to end
- `run-filebrowser-lite-wasi-example.sh` to run the Wasmtime example end to end

The WASI example now tracks `github.com/enbop/filebrowser-lite` and downloads:

```bash
https://github.com/enbop/filebrowser-lite/releases/latest/download/filebrowser-lite-wasi.wasm
```

It expects a `wasmtime serve` compatible component and serves the embedded File Browser frontend from a single `.wasm`.

From the repository root:

```bash
cd fungi
bash examples/service-manifests/run-filebrowser-example.sh
bash examples/service-manifests/run-filebrowser-lite-wasi-example.sh

# optional: override the demo port used by the helper scripts
SERVICE_PORT=28182 bash examples/service-manifests/run-filebrowser-lite-wasi-example.sh
```