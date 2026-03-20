<h1 align="center">
  <br>
  <a href="https://github.com/enbop/fungi"><img src="https://raw.githubusercontent.com/enbop/fungi/master/assets/FullLogo_Transparent_NoBuffer.png" alt="Fungi logo" title="Fungi logo" width="150"></a>
  <br>
  <br>
  Fungi
  <br>
</h1>

<p align="center" style="font-size: 1.2rem;">Build a private P2P device network for capability-first services</p>

With Fungi, you can securely connect your own devices, run container or WASI services under explicit runtime policy, control remote peers from CLI or GUI, and open published services locally.

This repository contains the **Core Daemon** and **CLI** tools.

> **Looking for the GUI?**
> Check out **[fungi-app](https://github.com/enbop/fungi-app)**, the official Flutter-based graphical interface for Fungi.
>
> **Need help or want to follow updates?**
> Join the **[Fungi Discord](https://discord.gg/A2vUXXB726)**.

## Key Features

*   **P2P Connectivity**: Built on [rust-libp2p](https://github.com/libp2p/rust-libp2p), supporting automatic NAT traversal and mDNS discovery.
*   **Secure**: End-to-end encryption with PeerID-based authentication.
*   **Capability-First Services**: Run Docker-compatible container services and WASI services within explicit path and port boundaries.
*   **Remote Service Workflow**: Use `peer`, `catalog`, and `access` to control remote nodes and open their published web apps locally.
*   **gRPC Interface**: The daemon exposes a gRPC API, allowing any client (CLI, GUI, scripts) to control it.
*   **Modular architecture:**
    *   **`fungi-daemon`**: The background service that handles P2P networking and manage services.
    *   **`fungi-cli`**: A command-line tool to interact with the daemon via gRPC.

> **Note on file transfer**
> The older FTP/WebDAV-style file transfer path is being gradually deprecated in favor of service-based workflows.

## Download
macOS / Linux quick install:

```bash
curl -fsSL https://fungi.rs/install.sh | sh
```

- Or download from [GitHub Releases](https://github.com/enbop/fungi/releases/latest)
- Or see the [full install guide](https://fungi.rs/docs/install)


## Documentation

Start with the beginner quick starts:

- [3 Minutes: Build Your Private P2P Network](https://fungi.rs/docs/quick-start/private-p2p-network)
- [2 Minutes: Run a Remote Sandbox App Locally](https://fungi.rs/docs/quick-start/remote-sandbox-app)

Full documentation: [fungi.rs/docs](https://fungi.rs/docs/intro).

Recommended starting points:

- [Fungi CLI Guide](https://fungi.rs/docs/cli-service-quick-start)
- [Remote Service Control](https://fungi.rs/docs/remote-service-control)
- [Services And Runtimes](https://fungi.rs/docs/service-manifests)

## Build from Source

See the install guide for source build instructions: [fungi.rs/docs/install](https://fungi.rs/docs/install).

## Platform Support

| Platform | Status |
|----------|--------|
| macOS    | ✅ Ready |
| Windows  | ✅ Ready |
| Linux    | ✅ Ready |
| Android  | ✅ Ready |
| iOS      | 🚧 In progress |

## Contributing

We welcome all contributions:
- 🐛 Bug reports and fixes
- ✨ New features
- 📖 Documentation
- 🎨 UI improvements

## License

Apache License 2.0
