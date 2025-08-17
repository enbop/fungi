use std::sync::OnceLock;

static MOBILE_DEVICE_NAME: OnceLock<String> = OnceLock::new();

pub fn init_mobile_device_name(name: String) {
    if MOBILE_DEVICE_NAME.get().is_some() {
        log::warn!("Mobile device name has already been initialized.");
        return;
    }
    MOBILE_DEVICE_NAME.set(name).unwrap();
}

pub fn get_mobile_device_name() -> Option<String> {
    MOBILE_DEVICE_NAME.get().cloned()
}
