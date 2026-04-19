pub const STABLE_CHANNEL: &str = "stable";
pub const NIGHTLY_CHANNEL: &str = "nightly";

pub const STABLE_FUNGI_DIR: &str = ".fungi";
pub const NIGHTLY_FUNGI_DIR: &str = ".fungi-nightly";

pub const STABLE_RPC_ADDRESS: &str = "127.0.0.1:5405";
pub const NIGHTLY_RPC_ADDRESS: &str = "127.0.0.1:5406";

pub fn dist_channel() -> &'static str {
    match option_env!("FUNGI_DIST_CHANNEL").unwrap_or(NIGHTLY_CHANNEL) {
        NIGHTLY_CHANNEL | "dev" => NIGHTLY_CHANNEL,
        STABLE_CHANNEL | "release" | "" => STABLE_CHANNEL,
        _ => NIGHTLY_CHANNEL,
    }
}

pub fn is_nightly() -> bool {
    dist_channel() == NIGHTLY_CHANNEL
}

pub fn default_fungi_dir_name() -> &'static str {
    if is_nightly() {
        NIGHTLY_FUNGI_DIR
    } else {
        STABLE_FUNGI_DIR
    }
}

pub fn default_rpc_address() -> &'static str {
    if is_nightly() {
        NIGHTLY_RPC_ADDRESS
    } else {
        STABLE_RPC_ADDRESS
    }
}

pub fn build_commit() -> &'static str {
    option_env!("FUNGI_BUILD_COMMIT").unwrap_or("unknown")
}

pub fn build_time() -> &'static str {
    option_env!("FUNGI_BUILD_TIME").unwrap_or("unknown")
}
