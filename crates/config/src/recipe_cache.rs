use std::{
    path::{Component, Path, PathBuf},
    time::SystemTime,
};

use anyhow::{Context as _, Result, bail};
use serde::{Deserialize, Serialize};
use ulid::Ulid;

const RECIPE_CACHE_DIR: &str = "cache/recipes";
const OFFICIAL_SOURCE_DIR: &str = "official";
const LATEST_RELEASE_FILE: &str = "latest.json";

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct RecipeCache {
    #[serde(skip)]
    root_dir: PathBuf,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CachedOfficialRelease {
    pub release_version: String,
    pub updated_at: SystemTime,
}

impl RecipeCache {
    pub fn apply_from_dir(fungi_dir: &Path) -> Result<Self> {
        let root_dir = fungi_dir.join(RECIPE_CACHE_DIR).join(OFFICIAL_SOURCE_DIR);
        std::fs::create_dir_all(&root_dir).with_context(|| {
            format!(
                "failed to create recipe cache directory: {}",
                root_dir.display()
            )
        })?;
        Ok(Self { root_dir })
    }

    pub fn latest_release_version(&self) -> Result<Option<String>> {
        let path = self.root_dir.join(LATEST_RELEASE_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let raw = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read recipe cache metadata: {}", path.display()))?;
        let entry: CachedOfficialRelease = serde_json::from_str(&raw).with_context(|| {
            format!("failed to parse recipe cache metadata: {}", path.display())
        })?;
        Ok(Some(entry.release_version))
    }

    pub fn set_latest_release_version(&self, release_version: impl Into<String>) -> Result<()> {
        std::fs::create_dir_all(&self.root_dir).with_context(|| {
            format!(
                "failed to create recipe cache directory: {}",
                self.root_dir.display()
            )
        })?;
        let entry = CachedOfficialRelease {
            release_version: release_version.into(),
            updated_at: SystemTime::now(),
        };
        let raw = serde_json::to_string_pretty(&entry)?;
        let path = self.root_dir.join(LATEST_RELEASE_FILE);
        std::fs::write(&path, raw)
            .with_context(|| format!("failed to write recipe cache metadata: {}", path.display()))
    }

    pub fn index_path(&self, release_version: &str) -> PathBuf {
        self.release_dir(release_version).join("index.json")
    }

    pub fn asset_path(&self, release_version: &str, asset_name: &str) -> Result<PathBuf> {
        Ok(self
            .assets_dir(release_version)
            .join(validate_asset_name(asset_name)?))
    }

    pub fn write_index(&self, release_version: &str, bytes: &[u8]) -> Result<PathBuf> {
        let path = self.index_path(release_version);
        self.write_bytes(&path, bytes)?;
        Ok(path)
    }

    pub fn write_asset(
        &self,
        release_version: &str,
        asset_name: &str,
        bytes: &[u8],
    ) -> Result<PathBuf> {
        let path = self.asset_path(release_version, asset_name)?;
        self.write_bytes(&path, bytes)?;
        Ok(path)
    }

    pub fn write_resolved_manifest(
        &self,
        release_version: &str,
        recipe_id: &str,
        service_name: &str,
        manifest_yaml: &str,
    ) -> Result<PathBuf> {
        let file_name = format!(
            "{}-{}-{}.yaml",
            sanitize_file_component(recipe_id),
            sanitize_file_component(service_name),
            Ulid::new()
        );
        let path = self.resolved_dir(release_version).join(file_name);
        self.write_bytes(&path, manifest_yaml.as_bytes())?;
        Ok(path)
    }

    pub fn ensure_asset_dir(&self, release_version: &str) -> Result<PathBuf> {
        let dir = self.assets_dir(release_version);
        std::fs::create_dir_all(&dir).with_context(|| {
            format!(
                "failed to create recipe asset cache directory: {}",
                dir.display()
            )
        })?;
        Ok(dir)
    }

    fn release_dir(&self, release_version: &str) -> PathBuf {
        self.root_dir.join(release_version)
    }

    fn assets_dir(&self, release_version: &str) -> PathBuf {
        self.release_dir(release_version).join("assets")
    }

    fn resolved_dir(&self, release_version: &str) -> PathBuf {
        self.release_dir(release_version).join("resolved")
    }

    fn write_bytes(&self, path: &Path, bytes: &[u8]) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create recipe cache directory: {}",
                    parent.display()
                )
            })?;
        }
        std::fs::write(path, bytes)
            .with_context(|| format!("failed to write recipe cache file: {}", path.display()))
    }
}

pub fn validate_asset_name(asset_name: &str) -> Result<&str> {
    if asset_name.is_empty() {
        bail!("recipe asset name cannot be empty");
    }
    if asset_name.contains('/') || asset_name.contains('\\') {
        bail!("recipe asset name must be a single file name: `{asset_name}`");
    }

    let mut components = Path::new(asset_name).components();
    match (components.next(), components.next()) {
        (Some(Component::Normal(_)), None) => Ok(asset_name),
        _ => bail!("recipe asset name must be a single file name: `{asset_name}`"),
    }
}

fn sanitize_file_component(value: &str) -> String {
    value
        .chars()
        .map(|ch| match ch {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => ch,
            _ => '-',
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn stores_latest_release_metadata_and_assets() {
        let dir = TempDir::new().unwrap();
        let cache = RecipeCache::apply_from_dir(dir.path()).unwrap();

        cache.set_latest_release_version("v0.3.1").unwrap();
        let index_path = cache
            .write_index("v0.3.1", br#"{"schemaVersion":1}"#)
            .unwrap();
        let manifest_path = cache
            .write_asset(
                "v0.3.1",
                "ssh-tunnel.manifest.yaml",
                b"apiVersion: fungi.rs/v1alpha1\n",
            )
            .unwrap();
        let resolved_path = cache
            .write_resolved_manifest(
                "v0.3.1",
                "ssh-tunnel",
                "home-ssh-tunnel",
                "apiVersion: fungi.rs/v1alpha1\n",
            )
            .unwrap();

        assert_eq!(
            cache.latest_release_version().unwrap().as_deref(),
            Some("v0.3.1")
        );
        assert!(index_path.exists());
        assert!(manifest_path.exists());
        assert!(resolved_path.exists());
    }

    #[test]
    fn rejects_asset_names_with_path_traversal() {
        let dir = TempDir::new().unwrap();
        let cache = RecipeCache::apply_from_dir(dir.path()).unwrap();

        let error = cache
            .write_asset("v0.3.1", "../escape.yaml", b"bad")
            .unwrap_err();

        assert!(
            error
                .to_string()
                .contains("recipe asset name must be a single file name")
        );
    }
}
