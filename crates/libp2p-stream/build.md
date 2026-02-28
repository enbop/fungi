```
cargo package --no-verify --allow-dirty -p libp2p-stream

mkdir -p ./dist

tar -xzf ../../target/package/libp2p-stream-0.4.0-alpha.crate \
    --strip-components=1 \
    -C ./dist
```