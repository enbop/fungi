fn main() {
    let target = std::env::var("TARGET").unwrap_or_default();

    // Skip proto generation for ARM64 Linux cross-compilation
    // Use pre-generated code instead to avoid protoc issues in cross environment
    if target == "aarch64-unknown-linux-gnu" {
        println!(
            "cargo:warning=Skipping proto generation for {}, using pre-generated code",
            target
        );
        return;
    }

    // For all other platforms, generate code
    generate_proto();
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
