<h1 align="center">
  <br>
  <a href="https://github.com/enbop/fungi"><img src="https://raw.githubusercontent.com/enbop/fungi/master/assets/FullLogo_Transparent_NoBuffer.png" alt="Fungi logo" title="Fungi logo" width="150"></a>
  <br>
  <br>
  Fungi
  <br>
</h1>

<p align="center" style="font-size: 1.2rem;">Turn Multiple Devices Into One Unified System</p>
<p align="center" style="font-size: 1rem;">A platform built for seamless multi-device integration</p>
<p align="center" style="font-size: 0.9rem; color: #666;">File Transfer • Port Forwarding • Cross-Device Integration (Coming Soon)</p>

<p align="center">
  <a href="docs/README_zh_CN.md">简体中文</a> •
  <a href="docs/README_ja.md">日本語</a>
</p>

<div align="center">
  <img src="assets/fungi-home-file-transfer.png" alt="File Transfer Interface" width="250" style="margin: 10px; border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2);">
  <img src="assets/fungi-data-tunnel.png" alt="Data Tunnel Interface" width="250" style="margin: 10px; border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2);">
</div>

<hr/>

## What is Fungi?

Fungi lets you securely connect and manage multiple devices through P2P connections. No server can see your data - everything is encrypted end-to-end between your devices.

Built with Rust (using [rust-libp2p](https://github.com/libp2p/rust-libp2p) for p2p) and Flutter for cross-platform UI.

## Why Fungi?

🚀 **Zero Configuration P2P** - Direct device-to-device connections with automatic NAT traversal, no public IP required

🛡️ **Security First** - End-to-end encryption with PeerID-based authentication and whitelist access control

🌐 **Gateway Architecture** - Turn any device into a gateway for services and files across your network

⚡ **Works Everywhere** - Local network auto-discovery via mDNS, seamless internet connectivity with relay fallback

🔧 **Self-hosted Ready** - Use our free relay server or deploy your own

📦 **Lightweight NAS** - Transform any device into a personal cloud storage with WebDAV/FTP mounting

🎯 **Cross-Platform** - Support for desktop (Windows/macOS/Linux), mobile (Android), ARM devices (Raspberry Pi, Orange Pi...)

### What You Can Do

📁 **File Transfer**
- Mount remote folders as local drives (FTP/WebDAV)
- Access files from any device like a lightweight NAS

🔗 **Port Forwarding** 
- Forward SSH, RDP, and any TCP service without VPS
- Secure tunneling between devices

🔮 **Cross-Device Integration** *(Coming Soon)*
- Remote computing and command execution
- WASI sandbox for cross-platform app deployment

## How It Works

**Local Network**: Devices automatically discover each other via mDNS - no setup needed.

**Internet**: Attempts NAT hole punching for direct P2P connections. If successful, data flows directly between devices; otherwise uses relay server. All traffic is end-to-end encrypted - relay server only sees encrypted data packets. By default, we provide a relay server. You can also set up your [own relay server](https://fungi.rs/docs/self_hosted_relay).

## Download
[Get the latest release](https://github.com/enbop/fungi/releases):

Available in two versions:
- **fungi-cli**: Command-line interface for terminal users
- **fungi-app**: Graphical user interface with Flutter UI

## Quick Start (fungi-app)

**Prerequisites**: 
1. Launch `Fungi App` on both devices
2. Click on each device's `PeerID` at the top of the app to automatically copy it and save them
   - **Tip**: You can also use "Select from Local Devices (mDNS)" to quickly choose devices that are currently online in the same local network

> File Transfer and Port Forwarding are independent features. You can use either one without the other based on your needs.

---

### 📁 File Transfer Example: Device A accessing files on Device B

**Use Case**: Access files from one device on another device through FTP/WebDAV.

**On Device B (File Server):**
1. Navigate to **File Transfer > File Server > Incoming Allowed Peers**
2. Add Device A's `PeerID` to the allowed list
3. Set **Shared Directory** to the folder you want to share (e.g., `/tmp`)
4. Enable **File Server State**

**On Device A (File Client):**
1. Go to **File Transfer > Remote File Access > Add Remote Device**
2. Add Device B's PeerID and assign an alias

**Access Files:**
The FTP/WebDAV addresses are displayed on home page. 
Use any FTP or WebDAV client on Device A to access the directory of Device B.
*(Both macOS and Windows built-in file managers can mount WebDAV as a drive)*

---

### 🔗 Port Forwarding Example: Accessing Device B's service from Device A

**Use Case**: Access services running on one device from another device through port tunneling.

**On Device B (Port Listening):**
1. Navigate to **Data Tunnel > Port Listening Rules**
2. Add the port you want to forward (e.g., `8080`)

**On Device A (Port Forwarding):**
1. Navigate to **Data Tunnel > Port Forwarding Rules**
2. Add Device B's PeerID and set up port mapping (e.g., local `9090` → remote `8080`)

**Access Service:**
Connect `localhost:9090` on Device A to reach the service running on Device B's port `8080`.

---

### Quick Start (fungi-cli)

See the [CLI Service Quick Start Guide](https://fungi.rs/docs/cli_service_quick_start).

## Build from Source

### Prerequisites

**All platforms require:**
- Rust toolchain
- Flutter SDK (for fungi-app only)
- Protocol Buffers compiler (protoc)

#### Install Dependencies

**Ubuntu/Debian:**
```bash
sudo apt-get install -y protobuf-compiler clang cmake ninja-build pkg-config libgtk-3-dev libayatana-appindicator3-dev
```

**macOS:**
```bash
brew install protobuf
```

**Windows:**

- Install build tools for aws-lc-rs [build dependencies](https://aws.github.io/aws-lc-rs/requirements/windows.html) (Ensure you have at least: C/C++ Compiler, CMake, NASM)

- Install protoc:
```powershell
choco install protoc
```

### Build fungi-cli

```bash
cargo build --release --bin fungi
```

Binary location: `./target/release/fungi`

### Build fungi-app
```bash
cd flutter_app
```

**Linux:**
```bash
flutter build linux --release
```

**macOS:**
```bash
flutter build macos --release
```

**Windows:**
```bash
flutter build windows --release
```

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
