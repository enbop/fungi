<h1 align="center">
  <br>
  <a href="https://github.com/enbop/fungi"><img src="https://raw.githubusercontent.com/enbop/fungi/master/assets/FullLogo_Transparent_NoBuffer.png" alt="Fungi logo" title="Fungi logo" width="150"></a>
  <br>
  <br>
  Fungi
  <br>
</h1>

<p align="center" style="font-size: 1.2rem;">Turn Multiple Devices Into One Unified System</p>

Fungi is a gateway for seamlessly exporting your services within a private P2P network.

With Fungi, you can securely `forward ports`, `transfer files`, `deploy apps`, or simply use it as a lightweight NAS.

This repository contains the **Core Daemon** and **CLI** tools.

> **Looking for the GUI?**
> Check out **[fungi-app](https://github.com/enbop/fungi-app)**, the official Flutter-based graphical interface for Fungi.

## Key Features

*   **P2P Connectivity**: Built on [rust-libp2p](https://github.com/libp2p/rust-libp2p), supporting automatic NAT traversal and mDNS discovery.
*   **Secure**: End-to-end encryption with PeerID-based authentication.
*   **File Transfer**: Mount remote folders as local drives (FTP/WebDAV).
*   **gRPC Interface**: The daemon exposes a gRPC API, allowing any client (CLI, GUI, scripts) to control it.
*   **Modular architecture:**
    *   **`fungi-daemon`**: The background service that handles P2P networking and manage services.
    *   **`fungi-cli`**: A command-line tool to interact with the daemon via gRPC.
*   **WASI Runtime**: (Experimental) WASI sandbox for cross-platform app deployment. [Learn more](https://fungi.rs/docs/wasi)

## Documentation

📚For full documentation, visit [fungi.rs/docs](https://fungi.rs/docs/intro).

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
