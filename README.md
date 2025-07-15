<h1 align="center">
  <br>
  <a href="https://github.com/enbop/fungi"><img src="https://raw.githubusercontent.com/enbop/fungi/master/assets/FullLogo_Transparent_NoBuffer.png" alt="Fungi logo" title="Fungi logo" width="150"></a>
  <br>
  <br>
  Fungi
  <br>
</h1>

<p align="center" style="font-size: 1.2rem;">Connect Your Devices Securely</p>
<p align="center" style="font-size: 1rem;">Easy file transfer, port forwarding, and more</p>

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

🔮 **Remote Execution** *(Coming Soon)*

## How It Works

**Local Network**: Devices automatically discover each other via mDNS - no setup needed.

**Internet**: Attempts NAT hole punching for direct P2P connections. If successful, data flows directly between devices; otherwise uses relay server. All traffic is end-to-end encrypted - relay server only sees encrypted data packets.

## Download
[Get the latest release](https://github.com/enbop/fungi/releases):

Available in two versions:
- **fungi-cli**: Command-line interface for terminal users
- **fungi-app**: Graphical user interface with Flutter UI

### Quick Start (fungi-app)

Let's say you have two devices: `Device A` and `Device B`, and you want `Device A` to access files on `Device B`.

#### Step 1: Setup Device A (Client)
1. Launch `Fungi App` on Device A
2. Copy Device A's `PeerID` from the status center at the top and save it

#### Step 2: Configure Device B (File Server)
1. Launch `Fungi App` on Device B
2. Navigate to **File Transfer > File Server > Incoming Allowed Peers**
   *(You can also find this setting in `Data Tunnel` and `Settings`)*
3. Add Device A's `PeerID` to Device B's `Incoming Allowed Peers` list
   *(Device B will now allow access from Device A)*

4. Set Device B's **Shared Directory** to the folder you want to share (e.g., `/tmp`)
5. Ensure the **File Server State** is enabled
6. Copy Device B's PeerID and save it

#### Step 3: Connect from Device A
1. On Device A, go to **File Transfer > Remote File Access > Add Remote Device**
2. Add Device B's PeerID and assign an alias for Device B

#### Step 4: Access Files
Now you can use your favorite FTP or WebDAV client to access the Remote File Access address. 
*(Both macOS and Windows built-in file managers can mount WebDAV as a readable/writable drive)*

> **Note**: More convenient mDNS local device discovery features are coming soon.

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
