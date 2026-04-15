fn main() {
    println!("cargo:rerun-if-changed=proto/fungi_daemon.proto");
    println!("cargo:rerun-if-changed=build.rs");

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
        .out_dir(&out_dir)
        .compile_protos(&["proto/fungi_daemon.proto"], &["proto"])
        .unwrap();

    format_generated_rs_files(&out_dir);
}

fn format_generated_rs_files(out_dir: &std::path::Path) {
    let rustfmt = std::env::var_os("RUSTFMT")
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| "rustfmt".into());
    let edition = std::env::var("CARGO_PKG_EDITION").unwrap_or_else(|_| "2024".to_string());

    let rs_files = match std::fs::read_dir(out_dir) {
        Ok(entries) => entries
            .filter_map(Result::ok)
            .map(|entry| entry.path())
            .filter(|path| path.extension().and_then(|ext| ext.to_str()) == Some("rs"))
            .collect::<Vec<_>>(),
        Err(error) => {
            println!(
                "cargo:warning=Failed to scan generated Rust files in {}: {}",
                out_dir.display(),
                error
            );
            return;
        }
    };

    if rs_files.is_empty() {
        return;
    }

    let status = std::process::Command::new(rustfmt)
        .arg("--edition")
        .arg(&edition)
        .args(&rs_files)
        .status();

    match status {
        Ok(status) if status.success() => {}
        Ok(status) => {
            println!(
                "cargo:warning=rustfmt exited with status {} while formatting generated protobuf code",
                status
            );
        }
        Err(error) => {
            println!(
                "cargo:warning=Failed to run rustfmt for generated protobuf code: {}",
                error
            );
        }
    }
}
