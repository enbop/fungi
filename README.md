<h1 align="center">
  <br>
  <a href="https://github.com/enbop/fungi"><img src="https://raw.githubusercontent.com/enbop/fungi/master/assets/FullLogo_Transparent_NoBuffer.png" alt="Fungi logo" title="Fungi logo" width="150"></a>
  <br>
  <br>
  Fungi
  <br>
</h1>

<p align="center" style="font-size: 1.2rem;">Effortless WASM deployment anywhere</p>
<p align="center" style="font-size: 1rem;">built on libp2p and WASI</p>

<hr/>

## Quickstart

### Run with Local Node

0. Build fungi from source:
```
cargo build --release

# Output binary file: target/release/fungi
```

1. Initialize fungi and start a fungi daemon:
```
$ target/release/fungi init
```
```
# Output:
Initializing Fungi...
Generating key pair...
Key pair generated Secp256k1:PublicKey { ... }
Key pair saved at $HOME/.fungi/.keys/keypair
Fungi initialized at $HOME/.fungi
```
```
$ target/release/fungi daemon
```
```
# Output
Starting Fungi daemon...
Fungi directory: "$HOME/.fungi"
Local Peer ID: ${PEER_ID}
Listening on "/ip4/127.0.0.1/tcp/${PORT}/p2p/${PEER_ID}"
Listening on "/ip4/x.x.x.x/tcp/${PORT}/p2p/${PEER_ID}"
...
```

2. Add some WASM applications to this node.

By default, the fungi WASI runtime will only search for and run WASM applications in the `$HOME/.fungi/root/bin` directory.

(Optional) You can quickly obtain a WASM application by building the Hello World example code provided in this project:
```
rustup target add wasm32-wasi
cargo build -p hello-fungi --release --target=wasm32-wasi

# Output .wasm file: target/wasm32-wasi/release/hello-fungi.wasm
```

Copy the WASM application to the directory:
```
mkdir -p $HOME/.fungi/root/bin
cp target/wasm32-wasi/release/hello-fungi.wasm $HOME/.fungi/root/bin/
```

3. In another shell, connect to this node and run the WASM application using the built-in `mush` tool:

```
$ target/release/fungi mush
Connecting to fungi daemon
Welcome to the Fungi!

# hello-fungi.wasm
Hello, Fungi!
```

### Run with Remote Node

Fungi enable mDNS by default, which will discover and register the device address automatically. You can connect to a LAN node using only the `Peer ID`.

1. On Device A within the same LAN, run the fungi daemon with the debug command to allow all inbound peers. **For demonstration only**.

```
# Run `fungi init` only once on one device
fungi init 

fungi daemon --debug-allow-all-peers true

# Copy the `Peer ID` from the output
```

2. On Device B within the same LAN, run the fungi daemon:

```
# Run `fungi init` only once on one device
fungi init
fungi daemon
```

3. On Device B, open another shell and use the `mush` tool to connect to Device A:
```
fungi mush -p ${PEER_ID_FROM_DEVICE_A}
```

## Platform Support

| Platform | Status |
|----------|--------|
| macOS    | ✅     |
| Windows  | ✅     |
| Linux    | ✅     |
| Android  | 🚧     |
| iOS      | 💤     |

## License

Apache License 2.0 - see the [LICENSE](LICENSE) file for details.
