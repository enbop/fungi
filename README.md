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

## What is Fungi?

Fungi is a modular project for distributed computing. It combines WASI (wasmtime) and libp2p. With Fungi, you can seamlessly run WASI applications on both local and remote devices. It allows you to securely connect to remote devices and perform tasks safely.

*Fungi is still in an experimental stage and welcomes contributions of any kind.*

## Quickstart

Fungi consists of two components:

- **fungi**: A WASI runtime for both local and remote devices.
- **fungi-daemon**: A libp2p service and a remote access service.

By default, all functionalities are bundled into a single binary -- `fungi`

#### Download fungi from Github releases:
[Github releases](https://github.com/enbop/fungi/releases)

#### Build fungi from source:
```
cargo build --release

# Output binary file: target/release/fungi
```

### Run with Local Node
1. Run fungi:
```
$ fungi
```

```
(output:)

Initializing Fungi...
Generating key pair...
Key pair generated Secp256k1:PublicKey { ... }
Key pair saved at $HOME/.fungi/.keys/keypair
Fungi initialized at $HOME/.fungi

Starting Fungi...
 # 
```

2. Add some WASM applications to this node.

By default, the fungi WASI runtime will only search for and run WASM applications in the `$HOME/.fungi/root/bin` directory.

(Optional) You can quickly obtain a WASM application by building the Hello World example provided in this project:
```
rustup target add wasm32-wasi
cargo build -p hello-fungi --release --target=wasm32-wasi

# Output .wasm file: target/wasm32-wasi/release/hello-fungi.wasm
```

Copy the WASM application to the directory:
```
cp target/wasm32-wasi/release/hello-fungi.wasm $HOME/.fungi/root/bin/
```

3. Return to the fungi cli, and run wasm:

```
...
Starting Fungi...
# hello-fungi.wasm
Hello, Fungi!
```

### Run with Remote Node

Fungi enable mDNS by default, which will discover and register LAN device address automatically. You can connect to a LAN node using only the `Peer ID`.

1. On Device A within the same LAN, run the fungi daemon with a **UNSAFE** debug flag to allow all inbound peers. **For demonstration only**.

```
fungi daemon --debug-allow-all-peers true

# Copy the `Peer ID` from the output
```

2. On Device B within the same LAN, run the fungi daemon:

```
fungi daemon
```

1. On Device B, open another shell and connect to Device A:
```
fungi -p ${PEER_ID_FROM_DEVICE_A}
```

## Platform Support

| Platform | Status |
|----------|--------|
| macOS    | ✅     |
| Windows  | ✅     |
| Linux    | ✅     |
| Android  | ✅     |
| iOS      | 💤     |
| Web      | 💤     |

*only support 64-bit, see: [Cranelift supports](https://docs.wasmtime.dev/stability-platform-support.html#compiler-support)

## Roadmap

TODO

## License

Apache License 2.0 - see the [LICENSE](LICENSE) file for details.
