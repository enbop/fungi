fn main() {
    #[cfg(not(all(target_os = "linux", target_arch = "aarch64")))]
    // Skip for Linux on ARM64 (cross-compilation issues)
    {
        generate_proto();
    }
}

fn generate_proto() {
    let cargo_manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap();
    let out_dir = std::path::PathBuf::from(cargo_manifest_dir)
        .join("src")
        .join("generated");
    tonic_prost_build::configure()
        .out_dir(out_dir)
        .compile_protos(&["proto/fungi_daemon.proto"], &["proto"])
        .unwrap();
}
