use std::path::{Path, PathBuf};

use crate::{STABLE_CHANNEL, dist_channel};

pub const STABLE_USER_ROOT_DIR: &str = "Fungi";
pub const DEV_USER_ROOT_DIR: &str = "FungiDev";
pub const USER_WORKSPACE_DIR: &str = "workspace";

// TODO(toolsets): add Tool artifact/appdata helpers when Tool execution lands.
// Do not create empty Toolset directories before the Tool feature exists.
#[derive(Debug, Clone)]
pub struct FungiPaths {
    fungi_home: PathBuf,
    user_root: PathBuf,
}

impl FungiPaths {
    pub fn from_fungi_home(fungi_home: impl Into<PathBuf>) -> Self {
        Self::from_fungi_home_for_channel(fungi_home, dist_channel())
    }

    pub fn from_fungi_home_for_channel(fungi_home: impl Into<PathBuf>, channel: &str) -> Self {
        let fungi_home = fungi_home.into();
        let user_root = user_root_for_fungi_home_and_channel(&fungi_home, channel);
        Self {
            fungi_home,
            user_root,
        }
    }

    pub fn fungi_home(&self) -> &Path {
        &self.fungi_home
    }

    pub fn user_root(&self) -> PathBuf {
        self.user_root.clone()
    }

    pub fn user_home(&self) -> PathBuf {
        self.user_root.join(USER_WORKSPACE_DIR)
    }

    pub fn services_root(&self) -> PathBuf {
        self.fungi_home.join("services")
    }

    pub fn appdata_root(&self) -> PathBuf {
        self.fungi_home.join("appdata")
    }

    pub fn service_appdata_root(&self) -> PathBuf {
        self.appdata_root().join("services")
    }

    pub fn service_appdata_dir(&self, local_service_id: &str) -> PathBuf {
        self.service_appdata_root().join(local_service_id)
    }

    pub fn artifacts_root(&self) -> PathBuf {
        self.fungi_home.join("artifacts")
    }

    pub fn service_artifacts_root(&self) -> PathBuf {
        self.artifacts_root().join("services")
    }

    pub fn service_artifacts_dir(&self, local_service_id: &str) -> PathBuf {
        self.service_artifacts_root().join(local_service_id)
    }
}

pub fn user_root_dir_name_for_channel(channel: &str) -> &'static str {
    if channel == STABLE_CHANNEL {
        STABLE_USER_ROOT_DIR
    } else {
        DEV_USER_ROOT_DIR
    }
}

pub fn user_root_for_fungi_home_and_channel(fungi_home: &Path, channel: &str) -> PathBuf {
    fungi_home
        .parent()
        .unwrap_or_else(|| Path::new("."))
        .join(user_root_dir_name_for_channel(channel))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{NIGHTLY_CHANNEL, STABLE_CHANNEL};

    #[test]
    fn stable_channel_uses_visible_fungi_root() {
        let paths = FungiPaths::from_fungi_home_for_channel("/tmp/device/.fungi", STABLE_CHANNEL);

        assert_eq!(paths.user_root(), PathBuf::from("/tmp/device/Fungi"));
        assert_eq!(
            paths.user_home(),
            PathBuf::from("/tmp/device/Fungi/workspace")
        );
    }

    #[test]
    fn nightly_channel_uses_visible_dev_root() {
        let paths =
            FungiPaths::from_fungi_home_for_channel("/tmp/device/.fungi-nightly", NIGHTLY_CHANNEL);

        assert_eq!(paths.user_root(), PathBuf::from("/tmp/device/FungiDev"));
        assert_eq!(
            paths.user_home(),
            PathBuf::from("/tmp/device/FungiDev/workspace")
        );
    }
}
