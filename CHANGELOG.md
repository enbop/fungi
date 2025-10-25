# Changelog

All notable changes to this project will be documented in this file.

## [0.4.0] - 2025-10-25

### Added

- **gRPC Support**: Daemon and control layers are now decoupled, communicating via gRPC protocol. Interact with fungi daemon using Fungi App, Fungi CLI, or any gRPC client
- **Enhanced CLI Commands**: Comprehensive CLI commands for daemon management
- **Experimental WASI Support**: Re-export `wasmtime-cli`'s `run` and `proxy` commands. You can now directly run WASM modules using fungi

## [0.3.4] - 2025-09-09

### Fixed

- Disabled wakelock for Android to conserve battery life
- Fixed auto-listening to relay servers when network environment changes [#10](https://github.com/enbop/fungi/issues/10)
- Resolved slow file transfer issue [#5](https://github.com/enbop/fungi/issues/5)

## [0.3.3] - 2025-08-20

### Added

- **Self-hosted relay server**: [#8](https://github.com/enbop/fungi/pull/8) Now you can simply use `fungi relay -p ${SERVER_PUBLIC_IP}` to start a self-hosted relay server.

### Fixed

- Relay could not be used in the Desktop GUI.

## [0.3.2] - 2025-08-19

### Added

- **Android Support**: Initial support for Android platform.

### Fixed

- Resolved issues with reading files via WebDAV/FTP.

## [0.3.1] - 2025-08-11

### Added
- **Select from Local Devices (mDNS)**: Quick device selection for devices currently online in the same local network
- **Select from Address Book**: Each manually added device is automatically saved to the Address Book for quick re-selection in future sessions
- **Minimize to tray**

### Fixed
- **Windows File Transfer**: Fixed incorrect file transfer path handling on Windows systems

## [0.3.0] - 2024-07-25

### Added
- **Complete Flutter UI**: Full graphical user interface with cross-platform desktop support (macOS, Windows, Linux)
- **File Transfer System**: End-to-end encrypted file sharing between devices with FTP/WebDAV mounting support
- **Port Forwarding**: End-to-end encrypted TCP port tunneling between devices
- **Default Relay Server**: Built-in relay server with automatic P2P fallback for improved connectivity
- **mDNS Device Discovery**: Automatic device discovery for local networks
- **Cross-platform Support**: Ready for macOS, Windows, and Linux

### Removed
- **WASI Module**: Temporarily removed for this release

### Changed
- **Release Format**: Now available in two versions:
  - `fungi-cli`: Command-line interface for server deployments
  - `fungi-app`: Graphical user interface for desktop users
