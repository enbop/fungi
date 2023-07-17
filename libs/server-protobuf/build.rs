fn main() {
    prost_build::compile_protos(&["src/server.proto"], &["src/"]).unwrap();
}
