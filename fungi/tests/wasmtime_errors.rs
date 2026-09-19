#![cfg(feature = "wasi")]

use std::{fs, process::Command};

#[test]
fn run_preserves_missing_command_export_and_import_causes() {
    let temp = tempfile::TempDir::new().unwrap();
    for (name, wat, cause) in [
        ("non-command", "(component)", "wasi:cli/run@"),
        (
            "missing-import",
            r#"(module (import "missing" "function" (func)) (func (export "_start")))"#,
            "unknown import: missing::function has not been defined",
        ),
    ] {
        let path = temp.path().join(format!("{name}.wat"));
        fs::write(&path, wat).unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_fungi"))
            .args(["run", "-Ccache=n"])
            .arg(&path)
            .current_dir(temp.path())
            .output()
            .unwrap();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr).replace('`', "");
        assert!(stderr.contains("failed to run main module"), "{stderr}");
        assert!(stderr.contains(cause), "{stderr}");
    }
}
