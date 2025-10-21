fn main() {
    tonic_prost_build::compile_protos("proto/fungi_daemon.proto").unwrap();
}
