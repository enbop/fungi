<h1 align="center">
  <br>
  <a href="https://github.com/enbop/fungi"><img src="https://raw.githubusercontent.com/enbop/fungi/master/assets/FullLogo_Transparent_NoBuffer.png" alt="Fungi logo" title="Fungi logo" width="150"></a>
  <br>
  <br>
  Fungi
  <br>
</h1>

<p align="center" style="font-size: 1.2rem;">
  Fungi turns your devices into a personal app platform.<br />
  Run apps on any device, and access them securely from anywhere.
</p>

<div align="center">
  <img src="https://fungi.rs/img/fungi-ping-dark.gif" alt="Connect two devices with Fungi" width="640">
  <br>
  <sub>Connect trusted devices into your private network.</sub>
</div>

<br>

<div align="center">
  <img src="https://fungi.rs/img/fungi-filebrowser-dark.gif" alt="Access a remote File Browser service with Fungi" width="640">
  <br>
  <sub>Open an app from another device as if it were running locally.</sub>
  <br>
  <sub><em>(Demo: <a href="https://github.com/enbop/filebrowser-lite">File Browser Lite</a>, a WASI fork of <a href="https://github.com/filebrowser/filebrowser">File Browser</a>, Apache-2.0.)</em></sub>
</div>

<br>

> **Need help or want to follow updates?**
> Join the **[Fungi Discord](https://discord.gg/A2vUXXB726)**.

## Key Features

- **Private Device Network**: Connect your devices with end-to-end encryption, directly when possible or through a relay when needed.
- **Explicit Device Trust**: Only devices you approve can initiate service access and management.
- **Sandboxed Apps as Services**: Run portable WebAssembly apps as services in the built-in WASI sandbox(Wasmtime), or use an optional constrained Docker backend.
- **Easy Service Access**: Access services across your device network without exposing them to the public internet.
- **Cross-Platform**: Run Fungi on macOS, Windows, Linux, and Android.

Use Fungi for private web apps, file access, APIs, and existing TCP or HTTP services.

## Download

macOS / Linux quick install:

```bash
curl -fsSL https://fungi.rs/install.sh | sh
```

- Or install with Homebrew using the official Fungi tap (macOS):

```bash
brew tap enbop/fungi
brew install fungi
```

- Or download from [GitHub Releases](https://github.com/enbop/fungi/releases/latest) (Windows/Linux/macOS/Android binaries available)
- Or see the [install and build guide](https://fungi.rs/docs/install)

## Documentation

Start with the quick starts:

- [3 Minutes: Build Your Private P2P Network](https://fungi.rs/docs/quick-start/private-p2p-network)
- [2 Minutes: Run a Remote Sandbox App Locally](https://fungi.rs/docs/quick-start/remote-sandbox-app)

Full documentation: [fungi.rs/docs](https://fungi.rs/docs/intro).

## Platform Support

| Platform | Status         |
| -------- | -------------- |
| macOS    | ✅ Ready       |
| Windows  | ✅ Ready       |
| Linux    | ✅ Ready       |
| Android  | ✅ Ready       |
| iOS      | 🚧 In progress |

## License

Apache License 2.0
