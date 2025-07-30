# Fungi Service CLI Quick Start

This guide shows you how to set up Fungi CLI as a service for file sharing and port forwarding.

## Prerequisites

1. Have Fungi CLI binary ready
2. Know the PeerIDs of devices you want to connect with

> File Transfer and Port Forwarding are independent features. You can configure and use either one without the other based on your needs.

## Step 1: Initialize Configuration

First, initialize the configuration file (use `-f` to specify a custom path if needed):

```bash
./fungi init
```

This will create a configuration file at `~/.fungi/config.toml` and display the path in the output.

## Step 2: Configure the Service

Edit the configuration file:

```toml
[network]
listen_tcp_port = 0
listen_udp_port = 0
incoming_allowed_peers = [
	"16Uiu2****" # Add allowed PeerID
]

[tcp_tunneling.forwarding]
enabled = false # Enable if you want to forward remote ports to this device, not needed for this example
rules = []

[tcp_tunneling.listening]
enabled = true
rules = [
	{ host = "127.0.0.1", port = 22 } # Port to expose to remote devices (e.g., SSH)
]

[file_transfer.server]
enabled = true # Set to enable file server
shared_root_dir = "/tmp" # Change to the directory you want to share


# Below are optional configurations for file transfer client mode, not needed for this example
[file_transfer.proxy_ftp]
enabled = false # Enable if you want to access files on remote devices from this device, not needed for this example
host = "127.0.0.1"
port = 2121

[file_transfer.proxy_webdav]
enabled = false # Enable if you want to access files on remote devices from this device, not needed for this example
host = "127.0.0.1"
port = 8181

[file_transfer]
client = [] # Add client config if you want to access files on remote devices from this device, not needed for this example
```

## Key Configuration Options

### Allow Remote Access
```toml
incoming_allowed_peers = [
	"16Uiu2****" # Add allowed PeerID
]
```
Add the PeerID of your trusted devices. These devices will be able to access your current device.

### Port Forwarding
```toml
[tcp_tunneling.listening]
enabled = true
rules = [
	{ host = "127.0.0.1", port = 22 }  # Port to expose to remote devices (e.g., SSH)
]
```
Add the ports you want to make accessible to remote devices.

### File Sharing
```toml
[file_transfer.server]
enabled = true # Set to enable file server
shared_root_dir = "/tmp"  # Change to the directory you want to share
```
Set `shared_root_dir` to the directory you want to share.

## Step 3: Start the Service and Get PeerID

Run the fungi daemon with your configuration (use `-f` to specify a custom path if needed):

```bash
./fungi daemon
```

You'll see this device's PeerID in the output.