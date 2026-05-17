<h1 align="center">
  <br>
  <a href="https://github.com/enbop/fungi"><img src="https://raw.githubusercontent.com/enbop/fungi/master/assets/FullLogo_Transparent_NoBuffer.png" alt="Fungi logo" title="Fungi logo" width="150"></a>
  <br>
  <br>
  Fungi
  <br>
</h1>

<p align="center" style="font-size: 1.2rem;">Build a private P2P device network for capability-first services</p>

With Fungi, you can securely connect your own devices, run container or WASI services under explicit runtime policy, 
manage remote services with simple commands, and access them locally without exposing them to the public internet.

| Demo |
| --- |
|Build a secure private P2P network with ease:<br /><img src="https://fungi.rs/assets/images/ping-ad101ea46e9e8bd25649d55fe290e801.gif" alt="Build a secure private P2P network" /> |
| Create a remote service and access it locally right away:<br /><img src="https://fungi.rs/assets/images/service-8e947b850359183aa2fc709388327e31.gif" alt="Create and start a remote service locally" /><br />(This demo shows creating a no-client file manager service running on a remote device, and accessing it with a browser locally) |
||


> **Need help or want to follow updates?**
> Join the **[Fungi Discord](https://discord.gg/A2vUXXB726)**.

## Key Features

*   **P2P Connectivity**: Built on [rust-libp2p](https://github.com/libp2p/rust-libp2p), supporting automatic NAT traversal and mDNS discovery.
*   **Secure**: End-to-end encryption with PeerID-based authentication.
*   **Fast and Lightweight**: Built in Rust, around 20 MB idle RAM, with support for macOS, Windows, Linux, and Android.
*   **Sandboxed Services**: Run sandboxed services with the built-in WASI runtime or an optional Docker backend.
*   **Simple Remote Service Control**: Use a few commands like `pull`, `start`, `stop`, and `remove` to manage remote services locally.
*   **Local Service Access**: open remote service endpoints locally without exposing them to the public internet.
*   **Modular architecture:**
    *   **`fungi-daemon`**: The background service that handles P2P networking and manage services.
    *   **`fungi-cli`**: A command-line tool to interact with the daemon via gRPC.
    *   **`fungi-app`**: (optional, external) An official GUI client for easier management (see [fungi-app](https://github.com/enbop/fungi-app)).

## Download
macOS / Linux quick install:

```bash
curl -fsSL https://fungi.rs/install.sh | sh
```

- Or install from Homebrew on macOS:

```bash
brew tap enbop/fungi
brew install fungi
```

- Or install the nightly Homebrew channel on macOS:

```bash
brew tap enbop/fungi
brew install fungi-nightly
```

- Or download from [GitHub Releases](https://github.com/enbop/fungi/releases/latest) (Windows/Linux/macOS/Android binaries available)
- Or see the [install and build guide](https://fungi.rs/docs/install)

## Documentation

Start with the quick starts:

- [3 Minutes: Build Your Private P2P Network](https://fungi.rs/docs/quick-start/private-p2p-network)
- [2 Minutes: Run a Remote Sandbox App Locally](https://fungi.rs/docs/quick-start/remote-sandbox-app)

Full documentation: [fungi.rs/docs](https://fungi.rs/docs/intro).

## Platform Support

| Platform | Status |
|----------|--------|
| macOS    | ✅ Ready |
| Windows  | ✅ Ready |
| Linux    | ✅ Ready |
| Android  | ✅ Ready |
| iOS      | 🚧 In progress |

## Development
Starting from 2026, the Fungi project actively adopts AI-assisted coding.

#### Code Quality
- Rust ensures safety in most cases
- Modular design
- Following TDD as much as possible

## License

Apache License 2.0
