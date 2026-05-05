use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result, bail};
use fungi_config::recipe_cache::{RecipeCache, validate_asset_name};
use serde::Deserialize;

use crate::ServiceManifestDocument;

const OFFICIAL_RECIPE_SOURCE_LABEL: &str = "enbop/fungi-service-recipes";
const OFFICIAL_RECIPE_LATEST_RELEASE_URL: &str =
    "https://github.com/enbop/fungi-service-recipes/releases/latest";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServiceRecipeRuntime {
    Docker,
    Wasmtime,
    Link,
}

#[derive(Debug, Clone)]
pub struct ServiceRecipeSummary {
    pub id: String,
    pub name: String,
    pub description: String,
    pub runtime: ServiceRecipeRuntime,
    pub stability: String,
    pub source_label: String,
    pub release_version: String,
}

#[derive(Debug, Clone)]
pub struct ServiceRecipeDetail {
    pub summary: ServiceRecipeSummary,
    pub tags: Vec<String>,
    pub homepage: Option<String>,
    pub cached_manifest_path: PathBuf,
    pub cached_readme_path: Option<PathBuf>,
    pub remote_manifest_url: String,
    pub remote_readme_url: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ResolvedServiceRecipe {
    pub detail: ServiceRecipeDetail,
    pub manifest_yaml: String,
    pub manifest_base_dir: PathBuf,
    pub resolved_manifest_path: PathBuf,
    pub warnings: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialRecipeIndex {
    recipes: Vec<OfficialRecipeRecord>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct OfficialRecipeRecord {
    id: String,
    name: String,
    description: String,
    runtime: String,
    manifest_asset: String,
    readme_asset: Option<String>,
    homepage: Option<String>,
    #[serde(default)]
    tags: Vec<String>,
    stability: String,
}

struct LoadedOfficialRecipeIndex {
    release_version: String,
    index: OfficialRecipeIndex,
    cache: RecipeCache,
}

pub async fn list_official_service_recipes(
    fungi_dir: &Path,
    refresh: bool,
) -> Result<Vec<ServiceRecipeSummary>> {
    let loaded = load_official_recipe_index(fungi_dir, refresh).await?;
    loaded
        .index
        .recipes
        .iter()
        .map(|recipe| build_recipe_summary(recipe, &loaded.release_version))
        .collect()
}

pub async fn get_official_service_recipe(
    fungi_dir: &Path,
    recipe_id: &str,
    refresh: bool,
) -> Result<ServiceRecipeDetail> {
    let loaded = load_official_recipe_index(fungi_dir, refresh).await?;
    let recipe = loaded.find_recipe(recipe_id)?;
    build_recipe_detail(&loaded, recipe).await
}

pub async fn resolve_official_service_recipe(
    fungi_dir: &Path,
    recipe_id: &str,
    service_name: Option<&str>,
    refresh: bool,
) -> Result<ResolvedServiceRecipe> {
    let loaded = load_official_recipe_index(fungi_dir, refresh).await?;
    let recipe = loaded.find_recipe(recipe_id)?;
    let detail = build_recipe_detail(&loaded, recipe).await?;
    let manifest_yaml =
        std::fs::read_to_string(&detail.cached_manifest_path).with_context(|| {
            format!(
                "failed to read cached recipe manifest: {}",
                detail.cached_manifest_path.display()
            )
        })?;
    let mut manifest_doc: ServiceManifestDocument = serde_yaml::from_str(&manifest_yaml)
        .with_context(|| {
            format!(
                "failed to parse recipe manifest: {}",
                detail.cached_manifest_path.display()
            )
        })?;
    manifest_doc.metadata.name = resolved_service_name(recipe, service_name);
    let resolved_manifest_yaml = serde_yaml::to_string(&manifest_doc)
        .context("failed to serialize resolved recipe manifest")?;

    let resolved_manifest_path = loaded.cache.write_resolved_manifest(
        &loaded.release_version,
        &recipe.id,
        &manifest_doc.metadata.name,
        &resolved_manifest_yaml,
    )?;
    let manifest_base_dir = loaded.cache.ensure_asset_dir(&loaded.release_version)?;

    Ok(ResolvedServiceRecipe {
        detail,
        manifest_yaml: resolved_manifest_yaml,
        manifest_base_dir,
        resolved_manifest_path,
        warnings: Vec::new(),
    })
}

impl LoadedOfficialRecipeIndex {
    fn find_recipe(&self, recipe_id: &str) -> Result<&OfficialRecipeRecord> {
        self.index
            .recipes
            .iter()
            .find(|recipe| recipe.id == recipe_id)
            .ok_or_else(|| anyhow::anyhow!("unknown recipe `{recipe_id}`"))
    }
}

fn build_recipe_summary(
    recipe: &OfficialRecipeRecord,
    release_version: &str,
) -> Result<ServiceRecipeSummary> {
    Ok(ServiceRecipeSummary {
        id: recipe.id.clone(),
        name: recipe.name.clone(),
        description: recipe.description.clone(),
        runtime: parse_recipe_runtime(&recipe.runtime)?,
        stability: recipe.stability.clone(),
        source_label: OFFICIAL_RECIPE_SOURCE_LABEL.to_string(),
        release_version: release_version.to_string(),
    })
}

async fn build_recipe_detail(
    loaded: &LoadedOfficialRecipeIndex,
    recipe: &OfficialRecipeRecord,
) -> Result<ServiceRecipeDetail> {
    let summary = build_recipe_summary(recipe, &loaded.release_version)?;
    let cached_manifest_path = ensure_cached_asset(
        &loaded.cache,
        &loaded.release_version,
        &recipe.manifest_asset,
    )
    .await?;
    let cached_readme_path = match &recipe.readme_asset {
        Some(readme_asset) => {
            Some(ensure_cached_asset(&loaded.cache, &loaded.release_version, readme_asset).await?)
        }
        None => None,
    };

    Ok(ServiceRecipeDetail {
        summary,
        tags: recipe.tags.clone(),
        homepage: recipe.homepage.clone(),
        cached_manifest_path,
        cached_readme_path,
        remote_manifest_url: release_asset_url(&loaded.release_version, &recipe.manifest_asset)?,
        remote_readme_url: recipe
            .readme_asset
            .as_ref()
            .map(|asset| release_asset_url(&loaded.release_version, asset))
            .transpose()?,
    })
}

async fn load_official_recipe_index(
    fungi_dir: &Path,
    refresh: bool,
) -> Result<LoadedOfficialRecipeIndex> {
    let cache = RecipeCache::apply_from_dir(fungi_dir)?;
    let release_version = if refresh {
        None
    } else {
        cache.latest_release_version()?
    };

    let release_version = match release_version {
        Some(release_version) if cache.index_path(&release_version).exists() => release_version,
        _ => refresh_latest_official_recipe_index(&cache).await?,
    };
    let index_path = cache.index_path(&release_version);
    let raw = std::fs::read_to_string(&index_path).with_context(|| {
        format!(
            "failed to read cached recipe index: {}",
            index_path.display()
        )
    })?;
    let index: OfficialRecipeIndex = serde_json::from_str(&raw).with_context(|| {
        format!(
            "failed to parse cached recipe index: {}",
            index_path.display()
        )
    })?;

    Ok(LoadedOfficialRecipeIndex {
        release_version,
        index,
        cache,
    })
}

async fn refresh_latest_official_recipe_index(cache: &RecipeCache) -> Result<String> {
    let latest_release_response = reqwest::get(OFFICIAL_RECIPE_LATEST_RELEASE_URL)
        .await
        .context("failed to resolve latest official recipe release")?
        .error_for_status()
        .context("latest official recipe release request failed")?;
    let release_version = release_version_from_release_page_url(latest_release_response.url())?;

    let response = reqwest::get(release_asset_url(&release_version, "index.json")?)
        .await
        .context("failed to fetch official recipe index")?
        .error_for_status()
        .context("official recipe index request failed")?;
    let bytes = response
        .bytes()
        .await
        .context("failed to read official recipe index response body")?;
    cache.write_index(&release_version, bytes.as_ref())?;
    cache.set_latest_release_version(release_version.clone())?;
    Ok(release_version)
}

async fn ensure_cached_asset(
    cache: &RecipeCache,
    release_version: &str,
    asset_name: &str,
) -> Result<PathBuf> {
    let path = cache.manifest_asset_path(release_version, asset_name)?;
    if path.exists()
        && path
            .metadata()
            .map(|metadata| metadata.len() > 0)
            .unwrap_or(false)
    {
        return Ok(path);
    }

    let response = reqwest::get(release_asset_url(release_version, asset_name)?)
        .await
        .with_context(|| format!("failed to fetch recipe asset `{asset_name}`"))?
        .error_for_status()
        .with_context(|| format!("recipe asset request failed for `{asset_name}`"))?;
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read recipe asset `{asset_name}`"))?;
    cache.write_asset(release_version, asset_name, bytes.as_ref())
}

fn release_asset_url(release_version: &str, asset_name: &str) -> Result<String> {
    let asset_name = validate_asset_name(asset_name)?;
    Ok(format!(
        "https://github.com/{OFFICIAL_RECIPE_SOURCE_LABEL}/releases/download/{release_version}/{asset_name}"
    ))
}

fn release_version_from_release_page_url(url: &reqwest::Url) -> Result<String> {
    let segments = url
        .path_segments()
        .map(|segments| segments.collect::<Vec<_>>())
        .unwrap_or_default();
    let Some(tag_index) = segments.iter().position(|segment| *segment == "tag") else {
        bail!("failed to determine recipe release version from {}", url);
    };
    let Some(release_version) = segments.get(tag_index + 1) else {
        bail!("failed to determine recipe release version from {}", url);
    };
    Ok((*release_version).to_string())
}

fn parse_recipe_runtime(value: &str) -> Result<ServiceRecipeRuntime> {
    match value.trim().to_ascii_lowercase().as_str() {
        "docker" => Ok(ServiceRecipeRuntime::Docker),
        "wasmtime" => Ok(ServiceRecipeRuntime::Wasmtime),
        "tcp" | "link" => Ok(ServiceRecipeRuntime::Link),
        other => bail!("unsupported recipe runtime `{other}`"),
    }
}

fn resolved_service_name(recipe: &OfficialRecipeRecord, service_name: Option<&str>) -> String {
    let value = service_name.unwrap_or_default().trim();
    if value.is_empty() {
        recipe.id.clone()
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_tcp_recipe_runtime_to_link() {
        assert_eq!(
            parse_recipe_runtime("tcp").unwrap(),
            ServiceRecipeRuntime::Link
        );
    }

    #[test]
    fn parses_release_version_from_release_page_url() {
        let url = reqwest::Url::parse(
            "https://github.com/enbop/fungi-service-recipes/releases/tag/v0.3.1",
        )
        .unwrap();

        assert_eq!(
            release_version_from_release_page_url(&url).unwrap(),
            "v0.3.1"
        );
    }
}
