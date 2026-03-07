# Service Manifest Examples

This directory contains runnable service manifest examples for the Fungi runtime-provider flow.

Documentation:

- https://fungi.rs/docs/service-manifests

Included examples:

- `filebrowser.service.yaml` for the Docker runtime
- `spore-box.service.yaml` for the Wasmtime runtime
- `run-filebrowser-example.sh` to run the Docker example end to end
- `run-spore-box-example.sh` to run the Wasmtime example end to end

From the repository root:

```bash
cd fungi
bash examples/service-manifests/run-filebrowser-example.sh
bash examples/service-manifests/run-spore-box-example.sh
```