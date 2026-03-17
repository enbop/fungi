<h1 align="center">
  <br>
  <a href="https://github.com/enbop/fungi"><img src="https://raw.githubusercontent.com/enbop/fungi/master/assets/FullLogo_Transparent_NoBuffer.png" alt="Fungi logo" title="Fungi logo" width="150"></a>
  <br>
  <br>
  Fungi
  <br>
</h1>

<p align="center" style="font-size: 1.2rem;">Build a private P2P device network for capability-first services</p>

Fungi is a private-device networking stack centered on encrypted P2P connectivity and capability-first services.

With Fungi, you can securely connect your own devices, run container or WASI services under explicit runtime policy, control remote peers from CLI or GUI, and open published services locally.

This repository contains the **Core Daemon** and **CLI** tools.

> **Looking for the GUI?**
> Check out **[fungi-app](https://github.com/enbop/fungi-app)**, the official Flutter-based graphical interface for Fungi.

## Key Features

*   **P2P Connectivity**: Built on [rust-libp2p](https://github.com/libp2p/rust-libp2p), supporting automatic NAT traversal and mDNS discovery.
*   **Secure**: End-to-end encryption with PeerID-based authentication.
*   **Capability-First Services**: Run Docker-compatible container services and WASI services within explicit path and port boundaries.
*   **Remote Service Workflow**: Use `peer`, `catalog`, and `access` to control remote nodes and open their published web apps locally.
*   **gRPC Interface**: The daemon exposes a gRPC API, allowing any client (CLI, GUI, scripts) to control it.
*   **Modular architecture:**
    *   **`fungi-daemon`**: The background service that handles P2P networking and manage services.
    *   **`fungi-cli`**: A command-line tool to interact with the daemon via gRPC.
*   **WASI Runtime**: Wasmtime-backed service runtime for WebAssembly components. Android support is not available yet. [Learn more](https://fungi.rs/docs/wasi)

> **Note on file transfer**
> The older FTP/WebDAV-style file transfer path is being gradually deprecated in favor of service-based workflows.

## Download
Download the latest binaries from [GitHub Releases](https://github.com/enbop/fungi/releases/latest).


## Documentation

📚For full documentation, visit [fungi.rs/docs](https://fungi.rs/docs/intro).

Recommended starting points:

- [CLI Quick Start](https://fungi.rs/docs/cli-service-quick-start)
- [Remote Service Control](https://fungi.rs/docs/remote-service-control)
- [Services And Runtimes](https://fungi.rs/docs/service-manifests)

## Build from Source

### Prerequisites

**All platforms require:**
- Rust toolchain
- Protocol Buffers compiler (protoc)

#### Install Dependencies

**Ubuntu/Debian:**
```bash
sudo apt-get install -y protobuf-compiler clang cmake ninja-build pkg-config
```

**macOS:**
```bash
brew install protobuf
```

**Windows:**
- Install build tools for aws-lc-rs [build dependencies](https://aws.github.io/aws-lc-rs/requirements/windows.html) (Ensure you have at least: C/C++ Compiler, CMake, NASM)
- Install protoc: `choco install protoc`

### Build



```bash
cargo build --release --bin fungi
```

Binary location: `./target/release/fungi`

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
