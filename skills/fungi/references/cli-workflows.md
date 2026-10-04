# CLI workflows

Use current `--help` output when it differs from this reference.

## Contents

- Install and daemon
- Device onboarding
- Recipes and service lifecycle
- Useful context

## Install and daemon

Official quick installer for macOS arm64, Linux x86_64, and Linux arm64:

```bash
curl -fsSL https://fungi.rs/install.sh | sh
```

Offer to inspect `https://fungi.rs/install.sh` before executing it. The default destination is `~/.local/bin`; ensure the standalone `fungi` CLI is on `PATH`, even when the desktop Fungi App is installed. Do not invoke a binary inside an app bundle. For unsupported platforms, use the assets at `https://github.com/enbop/fungi/releases/latest`. Do not build from source unless requested or no release fits.

Identify the CLI, then probe for an existing daemon before running `init`, starting a user service, or launching a foreground daemon:

```bash
fungi info build --json
fungi info version
fungi info rpc-address
fungi info config-path
```

`info build` describes the local CLI. The other commands query the daemon. If they succeed, reuse that daemon; do not start a second one. A desktop Fungi App and the CLI can share a compatible daemon in the default Fungi directory. Keep App-specific handling brief and keep agent operations CLI-first.

If versions differ or incompatibility is reported, record the CLI and daemon versions, RPC address, config path, and likely process owner first. A difference alone is not proof of incompatibility. Never stop, restart, kill, or close the daemon or Fungi App without explicit user permission. Treat a process as externally managed when ownership is unclear.

On Linux the installer may create `~/.config/systemd/user/fungi.service`:

```bash
systemctl --user start fungi.service
systemctl --user status fungi.service
journalctl --user -u fungi.service -n 200
```

Only when no daemon responds, run `fungi init` if needed and start the installed user service or run `fungi daemon` in a persistent foreground terminal. `fungi init` creates configuration and key material. Use `fungi init --upgrade-config` only to rewrite an older config intentionally.

## Device onboarding

On each CLI-capable device, run:

```bash
fungi info id
fungi device mdns
```

For an App-only target device, ask the user to copy its Device ID from Fungi App instead. Verify every Device ID through a trusted channel.

Save and inspect a device:

```bash
fungi device add NAME DEVICE_ID
fungi device add NAME DEVICE_ID --addr /ip4/ADDRESS/tcp/PORT
fungi device list
fungi device get NAME
```

Saving a device does not authorize it. Treat trust as high risk and do not place it in an unattended onboarding batch. On the device that would grant access:

```bash
fungi device add CONTROLLER_NAME CONTROLLER_DEVICE_ID
fungi device get CONTROLLER_NAME
fungi security show
```

Present the full Device ID, authorization direction, service-management capability, allowed host paths, persistence, and `fungi device untrust CONTROLLER_NAME` rollback command. Pause until the user explicitly approves this exact authorization. A general request to add or connect devices is not approval.

Only after approval, run:

```bash
fungi device trust CONTROLLER_NAME
fungi device trusted
```

Do not automate Fungi's confirmation input. Repeat the approval process independently in the opposite direction only when mutual access is wanted.

### Trust direction and verification

`fungi device trust REMOTE` authorizes REMOTE to initiate access to the device running that command. It does not grant that device permission to manage or ping REMOTE. Saving a device and establishing a transport connection do not grant this permission either.

In the examples below, `server` and `controller` are saved device names on the respective peers. Trust commands require the explicit authorization and native confirmation described above; the reverse row is optional.

| Intended access | Device granting access and trust command | Verify trust on | Run ping from | Run service access from |
| --- | --- | --- | --- | --- |
| Controller manages services on server | On server: `fungi device trust controller` | Server: `fungi device trusted` must list controller | Controller: `fungi ping server --count 4` | Controller: `fungi service inspect NAME@server --verbose`, then `fungi service connect NAME@server` when running |
| Server also manages services on controller | After separate approval, on controller: `fungi device trust server` | Controller: `fungi device trusted` must list server | Server: `fungi ping controller --count 4` | Server: `fungi service inspect NAME@controller --verbose`, then `fungi service connect NAME@controller` when running |

The first row alone is sufficient for controller-to-server operation. A failed reverse-direction ping can coexist with a healthy direct transport connection; check the authorization direction before diagnosing a network failure. Do not add reverse trust merely to make that ping succeed.

On the authorized controller, diagnose toward the target (`NAME` below is that target's saved device name):

```bash
fungi ping NAME --count 4
fungi connection overview --verbose
fungi connection streams --verbose
fungi connection relay-status --verbose
```

`--watch` continues until interrupted; use it only for an explicit monitoring request in a controllable terminal. Do not infer connectivity merely because a ping completed: inspect its connection and RTT rows.

## Recipes and service lifecycle

```bash
fungi service recipe list --refresh
fungi service recipe show RECIPE
fungi service apply NAME --recipe RECIPE --dry-run
fungi service apply NAME --recipe RECIPE --start
fungi service apply NAME@DEVICE --recipe RECIPE --dry-run
fungi service apply NAME@DEVICE --recipe RECIPE --start
```

The `--start` examples above request a running final state. For a custom file, replace `--recipe RECIPE` with its path. Use `NAME` for a local service or `NAME@DEVICE` for a remote service:

```bash
fungi service apply NAME ./service.fungi.md --dry-run
fungi service apply NAME@DEVICE ./service.fungi.md --start
```

### Choose the final service state

Inspect an existing target with `fungi service inspect NAME@DEVICE --verbose` before updating it. For the table below, `TARGET` is `NAME` locally or `NAME@DEVICE` remotely; `FILE` is the service file, or replace it with `--recipe RECIPE`.

| Situation and intended result | Command | Expected behavior on success |
| --- | --- | --- |
| First deployment, run immediately | `fungi service apply TARGET FILE --start` | Applies the definition and ensures the service is running |
| Update an already-running service | `fungi service apply TARGET FILE` | Preserves desired running state and restarts a managed workload |
| Update a stopped service, keep it stopped | `fungi service apply TARGET FILE` | Preserves desired stopped state |
| Update a stopped service, then start after inspection | `fungi service apply TARGET FILE`, inspect, then `fungi service start TARGET` | Separates applying the definition from the requested startup |

In the current CLI, `--start` means "ensure running after apply" and is valid on repeated applies, including running-service updates. A stopped service may also be updated and started in one command with `apply TARGET FILE --start` when that is the requested outcome. Check the installed CLI's `service apply --help` for older-version differences.

Apply preserves the stored desired state, which can differ from the observed state after a failure. An unchanged manifest does not make reapplying a running managed workload interruption-free: apply can still restart it. Services that forward an existing TCP endpoint do not manage or restart the external host process.

### Verify the result before retrying

Read the current CLI's `Manifest`, `Workload`, and `Final phase` output together with any errors. A manifest can be saved even when restart or final inspection fails. After mixed output, a failure, or a timeout, inspect the target and read bounded logs when available before retrying; do not infer rollback from an error or success from an "applied" message alone.

When the service should be running, use `service connect` from the authorized controller and check the actual published endpoint with its appropriate client (for example, an HTTP request for a web endpoint). A running phase or a local listener alone does not prove the application responds. If it should remain stopped, verify that state without starting it merely to test access. If state cannot be determined, report the uncertainty instead of blindly repeating apply or start.

Observe and control local or remote instances uniformly:

```bash
fungi service list --refresh
fungi service inspect NAME@DEVICE --verbose
fungi service logs NAME@DEVICE --tail 200
fungi service start NAME@DEVICE
fungi service stop NAME@DEVICE
fungi service remove NAME@DEVICE
```

Omit `@DEVICE` from these lifecycle commands for a local service.

Connect a published remote endpoint locally:

```bash
fungi service connect NAME@DEVICE
fungi service connect NAME@DEVICE ENTRY
fungi service connect NAME@DEVICE ENTRY --local-port PORT
fungi service disconnect NAME@DEVICE
```

Remote service management succeeds only when the target device trusts the controller. If a device is offline, `fungi service remove NAME@DEVICE --local-only` forgets cached state and does not remove the service from that device.

Ordinary RPC requests time out after 30 seconds; apply and pull operations time out after 300 seconds. A timeout from a state-changing command leaves the outcome uncertain: inspect the target with `service inspect`, `service list --refresh`, or bounded logs before retrying. Do not assume a timeout means the daemon rolled the operation back.

## Useful context

- Prefer the default Fungi directory so the CLI and desktop App can reach the same daemon. When isolation is intentional, put `-f /path/to/fungi` before the command and use it consistently.
- `fungi info build --json` describes the local binary without requiring the daemon.
- `fungi info config-path` and `fungi info rpc-address` identify the active daemon configuration and endpoint.
- `fungi security show` displays runtime boundaries. `security allow-path` expands host access and needs explicit user intent.
- Re-run the official installer to update, then compare the CLI and daemon versions. If the existing daemon is still on the prior version, identify its owner and end the update session by asking directly whether the user wants it restarted. Explain that daemon-managed services will be interrupted, and do not stop or restart anything until the user explicitly approves.
