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
  <a href="docs/README_zh.md">🇨🇳 简体中文</a> •
  <a href="docs/README_ja.md">🇯🇵 日本語</a>
</p>

<div align="center">
  <img src="assets/fungi-home-file-transfer.png" alt="File Transfer Interface" width="250" style="margin: 10px; border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2);">
  <img src="assets/fungi-data-tunnel.png" alt="Data Tunnel Interface" width="250" style="margin: 10px; border-radius: 8px; box-shadow: 0 4px 8px rgba(0,0,0,0.2);">
</div>

<hr/>

## What is Fungi?

Fungi lets you securely connect and manage multiple devices through P2P connections. No server can see your data - everything is encrypted end-to-end between your devices.

Built with Rust (using [rust-libp2p](https://github.com/libp2p/rust-libp2p) for p2p) and Flutter for cross-platform UI.

### What You Can Do

📁 **File Transfer**
- Mount remote folders as local drives (FTP/WebDAV)

🔗 **Port Forwarding** 

🔮 **Cross-Device Integration** *(Coming Soon)*

## How It Works

**Local Network**: Devices automatically discover each other via mDNS - no setup needed.

**Internet**: Attempts NAT hole punching for direct P2P connections. If successful, data flows directly between devices; otherwise uses relay server. All traffic is end-to-end encrypted - relay server only sees encrypted data packets. By default, we provide a relay server.

## Download
[Get the latest release](https://github.com/enbop/fungi/releases):

Available in two versions:
- **fungi-cli**: Command-line interface for terminal users
- **fungi-app**: Graphical user interface with Flutter UI

### Quick Start (fungi-app)

#### File Transfer Example

Let's say you have two devices: `Device A` and `Device B`, and you want `Device A` to access files on `Device B`.

#### Step 1: Launch and Get PeerIDs
1. Launch `Fungi App` on both devices
2. Click on each device's `PeerID` at the top of the app to automatically copy it and save them

#### Step 2: Configure Device B (File Server)
1. Navigate to **File Transfer > File Server > Incoming Allowed Peers**
2. Add Device A's `PeerID` to the allowed list
3. Set **Shared Directory** to the folder you want to share (e.g., `/tmp`) and enable **File Server State**

#### Step 3: Connect from Device A
1. Go to **File Transfer > Remote File Access > Add Remote Device**
2. Add Device B's PeerID and assign an alias

#### Step 4: Access Files
Use any FTP or WebDAV client to access the Remote File Access address.
*(Both macOS and Windows built-in file managers can mount WebDAV as a drive)*

#### Port Forwarding Example

To forward a port from Device B to Device A:

#### Step 1: Setup (same as above)
Launch apps and exchange PeerIDs between devices.

#### Step 2: Configure Device B (Port Server)
1. Navigate to **Data Tunnel > Port Listening Rules**
2. Add the port you want to forward (e.g., `8080`)

#### Step 3: Configure Device A (Port Client)  
1. Navigate to **Data Tunnel > Port Forwarding Rules**
2. Add Device B's PeerID and set up port mapping (e.g., local `9090` → remote `8080`)

#### Step 4: Access Service
Access `localhost:9090` on Device A to reach the service on Device B's port `8080`.

> **Note**: More convenient mDNS local device discovery features are coming soon.

### Quick Start (fungi-cli)

See the [CLI Service Quick Start Guide](docs/cli_service_quick_start.md).

## Build from Source

All platforms require Rust and Flutter to be installed.

### Build fungi-cli

Simply run:
```bash
cargo build --release --bin fungi
```
The binary will be located at:
```
./target/release/fungi
```

### Build fungi-app

#### Ubuntu
```
sudo apt-get install -y clang cmake ninja-build pkg-config libgtk-3-dev

cd flutter_app
flutter build linux --release
```

#### macOS
```bash
cd flutter_app
flutter build macos --release
```

#### Windows

Install aws-lc-rs [build dependencies](https://aws.github.io/aws-lc-rs/requirements/windows.html)

Ensure you have at least: C/C++ Compiler, CMake, NASM

```bash
cargo build --release -p rust_lib_fungi_app
flutter build windows --release
```

## Platform Support

| Platform | Status |
|----------|--------|
| macOS    | ✅ Ready |
| Windows  | ✅ Ready |
| Linux    | ✅ Ready |
| Android  | 🚧 In progress |
| iOS      | 🚧 In progress |

## Contributing

We welcome all contributions:
- 🐛 Bug reports and fixes
- ✨ New features
- 📖 Documentation
- 🎨 UI improvements

## License

Apache License 2.0 - see [LICENSE](LICENSE) for details.
