fn main() {
    tonic_build::compile_protos("src/server.proto").unwrap();
}
