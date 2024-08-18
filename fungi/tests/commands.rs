use clap::Parser;
use fungi::commands;

#[test]
fn init() {
    let tmp_dir = "debug-test-config";
    let args = fungi::commands::DaemonArgs::parse_from(["fungi-daemon", "-f", tmp_dir]);
    commands::init(&args).unwrap();
    std::fs::remove_dir_all(tmp_dir).unwrap();
}
