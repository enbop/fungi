// Compile as wasm32-wasip2 for the service lifecycle smoke test.
// The version is compiled into each artifact so updating the service exercises
// component download and replacement, not just an environment change.
fn main() {
    println!("fixture-{}", env!("FUNGI_FIXTURE_VERSION"));
    loop {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
