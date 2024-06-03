use clap::Parser;
use fungi::commands;

#[test]
fn init() {
    let tmp_dir = "debug-test-config";
    let args = fungi::commands::FungiArgs::parse_from(&["fungi", "-f", tmp_dir, "init"]);
    commands::init(&args);
    std::fs::remove_dir_all(tmp_dir).unwrap();
}
