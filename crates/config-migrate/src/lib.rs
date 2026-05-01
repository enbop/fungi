use anyhow::{Context, Result, bail};
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt, fs,
    path::{Path, PathBuf},
};
use ulid::Ulid;

pub const CURRENT_FUNGI_DIR_VERSION: u32 = 2;

const CONFIG_FILE: &str = "config.toml";
const BACKUP_ROOT_DIR: &str = "bk";
const STAGING_DIR_PREFIX: &str = ".fungi-migrate-staging-";
const APPLY_ROLLBACK_DIR_NAME: &str = ".apply-rollback";
const LEGACY_ADDRESS_BOOK_FILE: &str = "address_book.toml";
const DEVICES_FILE: &str = "devices.toml";
const LEGACY_SERVICE_STATE_FILE: &str = "services-state.json";
const SERVICES_ROOT_DIR: &str = "services";
const DATA_ROOT_DIR: &str = "data";
const CURRENT_SERVICE_STATE_SCHEMA_VERSION: u32 = 2;
const LEGACY_SERVICE_STATE_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DetectedVersion {
    MissingConfig,
    LegacyNoVersion,
    Version(u32),
}

impl DetectedVersion {
    fn backup_label(&self) -> String {
        match self {
            Self::MissingConfig => "missing-config".to_string(),
            Self::LegacyNoVersion => "legacy-no-version".to_string(),
            Self::Version(version) => format!("v{version}"),
        }
    }
}

impl fmt::Display for DetectedVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingConfig => write!(f, "missing config"),
            Self::LegacyNoVersion => write!(f, "legacy config without version"),
            Self::Version(version) => write!(f, "v{version}"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MigrationReport {
    pub source_version: DetectedVersion,
    pub target_version: u32,
    pub backup_dir: Option<PathBuf>,
    pub staging_dir: Option<PathBuf>,
    pub migrated_paths: Vec<PathBuf>,
    pub changed: bool,
}

impl MigrationReport {
    fn no_changes(source_version: DetectedVersion) -> Self {
        Self {
            source_version,
            target_version: CURRENT_FUNGI_DIR_VERSION,
            backup_dir: None,
            staging_dir: None,
            migrated_paths: Vec::new(),
            changed: false,
        }
    }
}

#[derive(Debug, Default)]
struct MigrationPlan {
    update_config: bool,
    migrate_address_book: bool,
    migrate_legacy_managed_services: bool,
    touched_paths: Vec<PathBuf>,
}

impl MigrationPlan {
    fn is_empty(&self) -> bool {
        self.touched_paths.is_empty()
    }
}

#[derive(Debug)]
struct MigrationTransaction {
    backup_dir: PathBuf,
    staging_dir: PathBuf,
}

#[derive(Debug)]
struct AppliedPath {
    relative_path: PathBuf,
    moved_live_to_rollback: bool,
    moved_staged_to_live: bool,
}

pub fn detect_source_version(fungi_dir: &Path) -> Result<DetectedVersion> {
    let config_path = fungi_dir.join(CONFIG_FILE);
    if !config_path.exists() {
        return Ok(DetectedVersion::MissingConfig);
    }

    let content = fs::read_to_string(&config_path).with_context(|| {
        format!(
            "Failed to read Fungi config file during migration detection: {}",
            config_path.display()
        )
    })?;
    detect_source_version_from_toml_str(&content)
}

pub fn migrate_if_needed(fungi_dir: &Path) -> Result<MigrationReport> {
    let source_version = detect_source_version(fungi_dir)?;
    let plan = build_migration_plan(fungi_dir, &source_version)?;

    match source_version.clone() {
        DetectedVersion::MissingConfig if plan.is_empty() => {
            Ok(MigrationReport::no_changes(source_version))
        }
        DetectedVersion::MissingConfig => bail!(
            "Found legacy Fungi side files without config.toml in {}. Initialize or restore the fungi-dir before migrating.",
            fungi_dir.display()
        ),
        DetectedVersion::Version(version)
            if version == CURRENT_FUNGI_DIR_VERSION && plan.is_empty() =>
        {
            Ok(MigrationReport::no_changes(source_version))
        }
        DetectedVersion::LegacyNoVersion | DetectedVersion::Version(CURRENT_FUNGI_DIR_VERSION) => {
            migrate_with_plan(fungi_dir, source_version, plan)
        }
        DetectedVersion::Version(version) => bail!(
            "Unsupported fungi-dir version {version}; no migration path to v{} is implemented yet",
            CURRENT_FUNGI_DIR_VERSION
        ),
    }
}

fn detect_source_version_from_toml_str(content: &str) -> Result<DetectedVersion> {
    let value: toml::Value = toml::from_str(content)
        .context("Failed to parse config.toml during migration detection")?;
    let Some(table) = value.as_table() else {
        bail!("config.toml root must be a TOML table for migration detection");
    };

    let Some(version) = table.get("version") else {
        return Ok(DetectedVersion::LegacyNoVersion);
    };

    let Some(version) = version.as_integer() else {
        bail!("config.toml version must be an integer for migration detection");
    };
    if version < 0 {
        bail!("config.toml version must not be negative for migration detection");
    }

    Ok(DetectedVersion::Version(version as u32))
}

fn build_migration_plan(
    fungi_dir: &Path,
    source_version: &DetectedVersion,
) -> Result<MigrationPlan> {
    let legacy_address_book_exists = fungi_dir.join(LEGACY_ADDRESS_BOOK_FILE).exists();
    let legacy_service_state_exists = fungi_dir.join(LEGACY_SERVICE_STATE_FILE).exists();

    if matches!(source_version, DetectedVersion::MissingConfig)
        && (legacy_address_book_exists || legacy_service_state_exists)
    {
        bail!(
            "Found legacy Fungi side files without config.toml in {}. Initialize or restore the fungi-dir before migrating.",
            fungi_dir.display()
        );
    }

    if legacy_service_state_exists {
        ensure_no_mixed_managed_service_layout(fungi_dir)?;
    }

    let mut touched_paths = BTreeSet::new();
    let update_config = config_requires_current_normalization(fungi_dir, source_version)?;
    if update_config {
        touched_paths.insert(PathBuf::from(CONFIG_FILE));
    }

    if legacy_address_book_exists {
        touched_paths.insert(PathBuf::from(LEGACY_ADDRESS_BOOK_FILE));
        touched_paths.insert(PathBuf::from(DEVICES_FILE));
    }

    if legacy_service_state_exists {
        touched_paths.insert(PathBuf::from(LEGACY_SERVICE_STATE_FILE));
        touched_paths.insert(PathBuf::from(SERVICES_ROOT_DIR));
        touched_paths.insert(PathBuf::from(DATA_ROOT_DIR));
    }

    Ok(MigrationPlan {
        update_config,
        migrate_address_book: legacy_address_book_exists,
        migrate_legacy_managed_services: legacy_service_state_exists,
        touched_paths: touched_paths.into_iter().collect(),
    })
}

fn config_requires_current_normalization(
    fungi_dir: &Path,
    source_version: &DetectedVersion,
) -> Result<bool> {
    if matches!(source_version, DetectedVersion::MissingConfig) {
        return Ok(false);
    }

    let config_path = fungi_dir.join(CONFIG_FILE);
    let content = fs::read_to_string(&config_path).with_context(|| {
        format!(
            "Failed to read Fungi config file while building migration plan: {}",
            config_path.display()
        )
    })?;
    let value: toml::Value = toml::from_str(&content)
        .context("Failed to parse config.toml while building migration plan")?;
    let Some(table) = value.as_table() else {
        bail!("config.toml root must be a TOML table while building migration plan");
    };

    if !table.contains_key("version") {
        return Ok(true);
    }

    let Some(runtime_table) = table.get("runtime").and_then(toml::Value::as_table) else {
        return Ok(false);
    };

    if runtime_table.contains_key("allowed_ports")
        || runtime_table.contains_key("allowed_port_ranges")
    {
        return Ok(true);
    }

    let legacy_services_path = normalize_fs_path(&fungi_dir.join(SERVICES_ROOT_DIR));
    let has_legacy_services_allowlist = runtime_table
        .get("allowed_host_paths")
        .and_then(toml::Value::as_array)
        .is_some_and(|paths| {
            paths.iter().any(|value| {
                value
                    .as_str()
                    .is_some_and(|raw| normalize_fs_path(Path::new(raw)) == legacy_services_path)
            })
        });

    Ok(has_legacy_services_allowlist)
}

fn ensure_no_mixed_managed_service_layout(fungi_dir: &Path) -> Result<()> {
    let services_root = fungi_dir.join(SERVICES_ROOT_DIR);
    if !services_root.exists() {
        return Ok(());
    }

    for entry in fs::read_dir(&services_root).with_context(|| {
        format!(
            "Failed to inspect services directory while planning migration: {}",
            services_root.display()
        )
    })? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }

        let path = entry.path();
        if path.join("service.yaml").exists() || path.join("state.json").exists() {
            bail!(
                "Unsupported mixed managed service layouts detected in {}. Legacy services-state.json cannot be merged automatically with current services/<local_service_id>/ entries yet.",
                services_root.display()
            );
        }
    }

    Ok(())
}

fn migrate_with_plan(
    fungi_dir: &Path,
    source_version: DetectedVersion,
    plan: MigrationPlan,
) -> Result<MigrationReport> {
    if plan.is_empty() {
        return Ok(MigrationReport::no_changes(source_version));
    }

    let transaction =
        MigrationTransaction::prepare(fungi_dir, &source_version, CURRENT_FUNGI_DIR_VERSION)?;

    copy_selected_paths(fungi_dir, &transaction.backup_dir, &plan.touched_paths)?;
    copy_selected_paths(fungi_dir, &transaction.staging_dir, &plan.touched_paths)?;

    if plan.update_config {
        migrate_config_toml_to_current(&transaction.staging_dir, fungi_dir)?;
    }
    if plan.migrate_address_book {
        migrate_legacy_address_book(&transaction.staging_dir)?;
    }
    if plan.migrate_legacy_managed_services {
        migrate_legacy_services_state(&transaction.staging_dir, fungi_dir)?;
    }

    validate_migrated_dir(&transaction.staging_dir, &plan)?;
    apply_staged_paths(&transaction.staging_dir, fungi_dir, &plan.touched_paths)?;
    fs::remove_dir_all(&transaction.staging_dir).with_context(|| {
        format!(
            "Failed to remove migration staging directory after success: {}",
            transaction.staging_dir.display()
        )
    })?;

    Ok(MigrationReport {
        source_version,
        target_version: CURRENT_FUNGI_DIR_VERSION,
        backup_dir: Some(transaction.backup_dir),
        staging_dir: None,
        migrated_paths: plan.touched_paths,
        changed: true,
    })
}

impl MigrationTransaction {
    fn prepare(
        fungi_dir: &Path,
        source_version: &DetectedVersion,
        target_version: u32,
    ) -> Result<Self> {
        let timestamp = Utc::now().format("%Y%m%dT%H%M%SZ").to_string();
        let backup_root = fungi_dir.join(BACKUP_ROOT_DIR);
        fs::create_dir_all(&backup_root).with_context(|| {
            format!(
                "Failed to create migration backup root directory: {}",
                backup_root.display()
            )
        })?;

        let backup_dir = backup_root.join(format!(
            "{timestamp}-from-{}-to-v{target_version}",
            source_version.backup_label()
        ));
        let staging_dir =
            fungi_dir.join(format!("{STAGING_DIR_PREFIX}{timestamp}-v{target_version}"));

        if backup_dir.exists() {
            bail!(
                "Refusing to overwrite existing migration backup directory: {}",
                backup_dir.display()
            );
        }
        if staging_dir.exists() {
            bail!(
                "Refusing to overwrite existing migration staging directory: {}",
                staging_dir.display()
            );
        }

        fs::create_dir_all(&backup_dir).with_context(|| {
            format!(
                "Failed to create migration backup directory: {}",
                backup_dir.display()
            )
        })?;
        fs::create_dir_all(&staging_dir).with_context(|| {
            format!(
                "Failed to create migration staging directory: {}",
                staging_dir.display()
            )
        })?;

        Ok(Self {
            backup_dir,
            staging_dir,
        })
    }
}

fn copy_selected_paths(
    source_root: &Path,
    target_root: &Path,
    relative_paths: &[PathBuf],
) -> Result<()> {
    for relative_path in relative_paths {
        if relative_path.is_absolute() {
            bail!(
                "Migration path must be relative to the fungi-dir root: {}",
                relative_path.display()
            );
        }

        let source = source_root.join(relative_path);
        if !source.exists() {
            continue;
        }

        copy_path_recursively(&source, &target_root.join(relative_path))?;
    }
    Ok(())
}

fn copy_path_recursively(source: &Path, target: &Path) -> Result<()> {
    let file_type = fs::symlink_metadata(source)
        .with_context(|| {
            format!(
                "Failed to read file metadata during migration: {}",
                source.display()
            )
        })?
        .file_type();

    if file_type.is_symlink() {
        bail!(
            "Symlinked paths are not supported by the migration tool yet: {}",
            source.display()
        );
    }

    if file_type.is_dir() {
        fs::create_dir_all(target).with_context(|| {
            format!(
                "Failed to create target directory during migration copy: {}",
                target.display()
            )
        })?;
        for entry in fs::read_dir(source).with_context(|| {
            format!(
                "Failed to read source directory during migration copy: {}",
                source.display()
            )
        })? {
            let entry = entry?;
            let target_path = target.join(entry.file_name());
            copy_path_recursively(&entry.path(), &target_path)?;
        }
        return Ok(());
    }

    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create parent directory during migration copy: {}",
                parent.display()
            )
        })?;
    }
    fs::copy(source, target).with_context(|| {
        format!(
            "Failed to copy file during migration from {} to {}",
            source.display(),
            target.display()
        )
    })?;
    Ok(())
}

fn migrate_config_toml_to_current(staging_root: &Path, fungi_dir: &Path) -> Result<()> {
    let config_path = staging_root.join(CONFIG_FILE);
    let content = fs::read_to_string(&config_path).with_context(|| {
        format!(
            "Failed to read config.toml from staging directory: {}",
            config_path.display()
        )
    })?;
    let mut value: toml::Value =
        toml::from_str(&content).context("Failed to parse config.toml in staging")?;
    let Some(table) = value.as_table_mut() else {
        bail!("config.toml root must be a TOML table");
    };

    table.insert(
        "version".to_string(),
        toml::Value::Integer(CURRENT_FUNGI_DIR_VERSION.into()),
    );

    if let Some(runtime_table) = table.get_mut("runtime").and_then(toml::Value::as_table_mut) {
        runtime_table.remove("allowed_ports");
        runtime_table.remove("allowed_port_ranges");
        remove_legacy_services_allowlist(runtime_table, fungi_dir);
    }

    let updated = toml::to_string_pretty(&value)
        .context("Failed to encode migrated config.toml from staging")?;
    fs::write(&config_path, updated).with_context(|| {
        format!(
            "Failed to write migrated config.toml into staging directory: {}",
            config_path.display()
        )
    })?;

    Ok(())
}

fn remove_legacy_services_allowlist(runtime_table: &mut toml::Table, fungi_dir: &Path) {
    let legacy_services_path = normalize_fs_path(&fungi_dir.join(SERVICES_ROOT_DIR));
    let Some(paths) = runtime_table
        .get_mut("allowed_host_paths")
        .and_then(toml::Value::as_array_mut)
    else {
        return;
    };

    paths.retain(|value| {
        !value
            .as_str()
            .is_some_and(|raw| normalize_fs_path(Path::new(raw)) == legacy_services_path)
    });
    if paths.is_empty() {
        runtime_table.remove("allowed_host_paths");
    }
}

fn migrate_legacy_address_book(staging_root: &Path) -> Result<()> {
    let address_book_path = staging_root.join(LEGACY_ADDRESS_BOOK_FILE);
    if !address_book_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&address_book_path).with_context(|| {
        format!(
            "Failed to read legacy address_book.toml from staging directory: {}",
            address_book_path.display()
        )
    })?;
    let address_book_value: toml::Value =
        toml::from_str(&content).context("Failed to parse legacy address_book.toml in staging")?;
    let peers = address_book_value
        .as_table()
        .and_then(|table| table.get("peers"))
        .and_then(toml::Value::as_array)
        .cloned()
        .unwrap_or_default();

    let devices_path = staging_root.join(DEVICES_FILE);
    let mut devices_value = if devices_path.exists() {
        let existing = fs::read_to_string(&devices_path).with_context(|| {
            format!(
                "Failed to read existing devices.toml from staging directory: {}",
                devices_path.display()
            )
        })?;
        toml::from_str(&existing).context("Failed to parse existing devices.toml in staging")?
    } else {
        toml::Value::Table(toml::map::Map::new())
    };

    let devices_array = ensure_toml_array_field(&mut devices_value, "devices")?;
    let mut existing_peer_ids = devices_array
        .iter()
        .filter_map(|value| value.as_table())
        .filter_map(|table| table.get("peer_id"))
        .filter_map(toml::Value::as_str)
        .map(ToOwned::to_owned)
        .collect::<BTreeSet<_>>();

    for peer in peers {
        let peer_table = peer
            .as_table()
            .ok_or_else(|| anyhow::anyhow!("legacy address_book peers must be TOML tables"))?;
        let peer_id = required_toml_string(peer_table, "peer_id")?;
        if existing_peer_ids.contains(peer_id) {
            continue;
        }

        devices_array.push(toml::Value::Table(legacy_peer_to_device_table(peer_table)?));
        existing_peer_ids.insert(peer_id.to_string());
    }

    let encoded = toml::to_string_pretty(&devices_value)
        .context("Failed to encode migrated devices.toml from staging")?;
    fs::write(&devices_path, encoded).with_context(|| {
        format!(
            "Failed to write migrated devices.toml into staging directory: {}",
            devices_path.display()
        )
    })?;

    fs::remove_file(&address_book_path).with_context(|| {
        format!(
            "Failed to remove legacy address_book.toml from staging directory: {}",
            address_book_path.display()
        )
    })?;
    Ok(())
}

fn ensure_toml_array_field<'a>(
    value: &'a mut toml::Value,
    key: &str,
) -> Result<&'a mut Vec<toml::Value>> {
    let table = value
        .as_table_mut()
        .ok_or_else(|| anyhow::anyhow!("expected TOML table while accessing '{key}'"))?;
    if !table.contains_key(key) {
        table.insert(key.to_string(), toml::Value::Array(Vec::new()));
    }

    table
        .get_mut(key)
        .and_then(toml::Value::as_array_mut)
        .ok_or_else(|| anyhow::anyhow!("expected TOML array for '{key}'"))
}

fn legacy_peer_to_device_table(peer_table: &toml::Table) -> Result<toml::Table> {
    let mut device_table = toml::map::Map::new();
    device_table.insert(
        "peer_id".to_string(),
        peer_table
            .get("peer_id")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("legacy address_book peer is missing peer_id"))?,
    );
    if let Some(alias) = peer_table.get("alias").cloned() {
        device_table.insert("name".to_string(), alias);
    }
    if let Some(hostname) = peer_table.get("hostname").cloned() {
        device_table.insert("hostname".to_string(), hostname);
    }
    device_table.insert("multiaddrs".to_string(), toml::Value::Array(Vec::new()));
    device_table.insert(
        "private_ips".to_string(),
        peer_table
            .get("private_ips")
            .cloned()
            .unwrap_or_else(|| toml::Value::Array(Vec::new())),
    );
    device_table.insert(
        "os".to_string(),
        peer_table
            .get("os")
            .cloned()
            .unwrap_or_else(|| toml::Value::String("Unknown".to_string())),
    );
    device_table.insert(
        "version".to_string(),
        peer_table
            .get("version")
            .cloned()
            .unwrap_or_else(|| toml::Value::String(String::new())),
    );
    if let Some(public_ip) = peer_table.get("public_ip").cloned() {
        device_table.insert("public_ip".to_string(), public_ip);
    }
    device_table.insert(
        "created_at".to_string(),
        peer_table
            .get("created_at")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("legacy address_book peer is missing created_at"))?,
    );
    device_table.insert(
        "last_connected".to_string(),
        peer_table
            .get("last_connected")
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("legacy address_book peer is missing last_connected"))?,
    );
    Ok(device_table)
}

fn required_toml_string<'a>(table: &'a toml::Table, key: &str) -> Result<&'a str> {
    table
        .get(key)
        .and_then(toml::Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("expected string field '{key}' in legacy TOML table"))
}

fn migrate_legacy_services_state(staging_root: &Path, live_root: &Path) -> Result<()> {
    let legacy_state_path = staging_root.join(LEGACY_SERVICE_STATE_FILE);
    if !legacy_state_path.exists() {
        return Ok(());
    }

    let content = fs::read_to_string(&legacy_state_path).with_context(|| {
        format!(
            "Failed to read legacy services-state.json from staging directory: {}",
            legacy_state_path.display()
        )
    })?;
    let legacy_state: LegacyServiceStateFile = serde_json::from_str(&content)
        .context("Failed to parse legacy services-state.json in staging")?;
    if legacy_state.schema_version != LEGACY_SERVICE_STATE_SCHEMA_VERSION {
        bail!(
            "Unsupported legacy service state schema version {} in {}",
            legacy_state.schema_version,
            legacy_state_path.display()
        );
    }

    let legacy_updated_at = normalize_optional(Some(legacy_state.updated_at))
        .unwrap_or_else(|| Utc::now().to_rfc3339());
    let services_root = staging_root.join(SERVICES_ROOT_DIR);
    let data_root = staging_root.join(DATA_ROOT_DIR);
    let mut allocated_local_service_ids = BTreeSet::new();

    for (service_name_key, persisted_service) in legacy_state.services {
        let service_name =
            normalize_non_empty(&persisted_service.manifest.name, "legacy manifest.name")?;
        if service_name_key != service_name {
            bail!(
                "Legacy services-state entry '{}' does not match manifest name '{}'",
                service_name_key,
                service_name
            );
        }

        let local_service_id = generate_unique_local_service_id(&mut allocated_local_service_ids)?;
        let old_service_data_dir = services_root.join(&service_name);
        let old_live_service_data_dir = live_root.join(SERVICES_ROOT_DIR).join(&service_name);
        let new_service_data_dir = data_root.join(&local_service_id);
        let new_live_service_data_dir = live_root.join(DATA_ROOT_DIR).join(&local_service_id);
        move_or_create_service_data_dir(&old_service_data_dir, &new_service_data_dir)?;

        let manifest_document = migrate_legacy_service_manifest(
            persisted_service.manifest,
            &old_live_service_data_dir,
            &new_live_service_data_dir,
        );
        let service_dir = services_root.join(&local_service_id);
        fs::create_dir_all(&service_dir).with_context(|| {
            format!(
                "Failed to create migrated managed service directory in staging: {}",
                service_dir.display()
            )
        })?;

        let manifest_yaml = serde_yaml::to_string(&manifest_document)
            .context("Failed to encode migrated managed service manifest YAML")?;
        fs::write(service_dir.join("service.yaml"), manifest_yaml).with_context(|| {
            format!(
                "Failed to write migrated managed service manifest in staging: {}",
                service_dir.join("service.yaml").display()
            )
        })?;

        let state_file = CurrentServiceStateFile {
            schema_version: CURRENT_SERVICE_STATE_SCHEMA_VERSION,
            local_service_id: local_service_id.clone(),
            updated_at: legacy_updated_at.clone(),
            desired_state: persisted_service.desired_state.into(),
        };
        let state_bytes = serde_json::to_vec_pretty(&state_file)
            .context("Failed to encode migrated managed service state.json")?;
        fs::write(service_dir.join("state.json"), state_bytes).with_context(|| {
            format!(
                "Failed to write migrated managed service state in staging: {}",
                service_dir.join("state.json").display()
            )
        })?;
    }

    fs::remove_file(&legacy_state_path).with_context(|| {
        format!(
            "Failed to remove legacy services-state.json from staging directory: {}",
            legacy_state_path.display()
        )
    })?;
    Ok(())
}

fn generate_unique_local_service_id(allocated: &mut BTreeSet<String>) -> Result<String> {
    for _ in 0..16 {
        let candidate = format!("svc_{}", Ulid::new().to_string().to_ascii_lowercase());
        if allocated.insert(candidate.clone()) {
            return Ok(candidate);
        }
    }
    bail!("failed to allocate unique local_service_id during migration")
}

fn move_or_create_service_data_dir(old_path: &Path, new_path: &Path) -> Result<()> {
    if new_path.exists() {
        bail!(
            "Refusing to overwrite migrated service data directory in staging: {}",
            new_path.display()
        );
    }

    if old_path.exists() {
        if let Some(parent) = new_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "Failed to create migrated service data parent directory in staging: {}",
                    parent.display()
                )
            })?;
        }
        fs::rename(old_path, new_path).with_context(|| {
            format!(
                "Failed to move legacy service data directory from {} to {}",
                old_path.display(),
                new_path.display()
            )
        })?;
        return Ok(());
    }

    fs::create_dir_all(new_path).with_context(|| {
        format!(
            "Failed to create migrated service data directory in staging: {}",
            new_path.display()
        )
    })
}

fn migrate_legacy_service_manifest(
    manifest: LegacyServiceManifest,
    old_service_data_dir: &Path,
    new_service_data_dir: &Path,
) -> CurrentServiceManifestDocument {
    let source = match manifest.source {
        LegacyServiceSource::Docker { image } => CurrentServiceManifestSource {
            image: Some(image),
            ..CurrentServiceManifestSource::default()
        },
        LegacyServiceSource::WasmtimeFile { component } => CurrentServiceManifestSource {
            file: Some(
                rewrite_legacy_managed_path(component, old_service_data_dir, new_service_data_dir)
                    .display()
                    .to_string(),
            ),
            ..CurrentServiceManifestSource::default()
        },
        LegacyServiceSource::WasmtimeUrl { url } => CurrentServiceManifestSource {
            url: Some(url),
            ..CurrentServiceManifestSource::default()
        },
    };

    CurrentServiceManifestDocument {
        api_version: "fungi.rs/v1alpha1".to_string(),
        kind: "ServiceManifest".to_string(),
        metadata: CurrentServiceManifestMetadata {
            name: manifest.name,
            labels: manifest.labels,
        },
        spec: CurrentServiceManifestSpec {
            runtime: manifest.runtime,
            source,
            expose: manifest.expose.map(|expose| CurrentServiceManifestExpose {
                enabled: true,
                transport: Some(CurrentServiceManifestExposeTransport {
                    kind: expose.transport.kind,
                }),
                usage: expose.usage.map(|usage| CurrentServiceManifestExposeUsage {
                    kind: usage.kind,
                    path: normalize_optional(usage.path),
                }),
                icon_url: normalize_optional(expose.icon_url),
                catalog_id: normalize_optional(expose.catalog_id),
            }),
            env: manifest.env,
            mounts: manifest
                .mounts
                .into_iter()
                .map(|mount| CurrentServiceManifestMount {
                    host_path: rewrite_legacy_managed_path(
                        mount.host_path,
                        old_service_data_dir,
                        new_service_data_dir,
                    )
                    .display()
                    .to_string(),
                    runtime_path: mount.runtime_path,
                })
                .collect(),
            ports: manifest
                .ports
                .into_iter()
                .map(|port| CurrentServiceManifestPort {
                    host_port: Some(CurrentServiceManifestHostPort::Fixed(port.host_port)),
                    service_port: port.service_port,
                    name: port.name,
                    protocol: port.protocol,
                })
                .collect(),
            command: manifest.command,
            entrypoint: manifest.entrypoint,
            working_dir: manifest.working_dir.map(|path| {
                rewrite_legacy_managed_path(
                    PathBuf::from(path),
                    old_service_data_dir,
                    new_service_data_dir,
                )
                .display()
                .to_string()
            }),
        },
    }
}

fn rewrite_legacy_managed_path(path: PathBuf, old_root: &Path, new_root: &Path) -> PathBuf {
    if path == old_root {
        return new_root.to_path_buf();
    }

    if let Ok(suffix) = path.strip_prefix(old_root) {
        return new_root.join(suffix);
    }

    path
}

fn validate_migrated_dir(staging_root: &Path, plan: &MigrationPlan) -> Result<()> {
    if plan.update_config {
        match detect_source_version(staging_root)? {
            DetectedVersion::Version(version) if version == CURRENT_FUNGI_DIR_VERSION => {}
            other => {
                bail!(
                    "Migration validation failed; expected v{} but found {}",
                    CURRENT_FUNGI_DIR_VERSION,
                    other
                )
            }
        }
    }

    if plan.migrate_address_book {
        if staging_root.join(LEGACY_ADDRESS_BOOK_FILE).exists() {
            bail!("Migration validation failed; legacy address_book.toml still exists in staging");
        }
        if !staging_root.join(DEVICES_FILE).exists() {
            bail!("Migration validation failed; devices.toml was not created in staging");
        }
    }

    if plan.migrate_legacy_managed_services && staging_root.join(LEGACY_SERVICE_STATE_FILE).exists()
    {
        bail!("Migration validation failed; legacy services-state.json still exists in staging");
    }

    Ok(())
}

fn apply_staged_paths(
    staging_root: &Path,
    live_root: &Path,
    migrated_paths: &[PathBuf],
) -> Result<()> {
    let rollback_root = staging_root.join(APPLY_ROLLBACK_DIR_NAME);
    fs::create_dir_all(&rollback_root).with_context(|| {
        format!(
            "Failed to create migration apply rollback directory in staging: {}",
            rollback_root.display()
        )
    })?;

    let mut applied_paths = Vec::new();
    for relative_path in migrated_paths {
        match apply_one_staged_path(staging_root, live_root, &rollback_root, relative_path) {
            Ok(applied_path) => applied_paths.push(applied_path),
            Err(error) => {
                if let Err(rollback_error) =
                    rollback_applied_paths(live_root, &rollback_root, &applied_paths)
                {
                    return Err(error).context(format!(
                        "Additionally failed to roll back migration apply: {rollback_error}"
                    ));
                }
                return Err(error);
            }
        }
    }

    Ok(())
}

fn apply_one_staged_path(
    staging_root: &Path,
    live_root: &Path,
    rollback_root: &Path,
    relative_path: &Path,
) -> Result<AppliedPath> {
    let staged_path = staging_root.join(relative_path);
    let live_path = live_root.join(relative_path);
    let rollback_path = rollback_root.join(relative_path);

    let live_exists = live_path.exists();
    let staged_exists = staged_path.exists();
    let mut moved_live_to_rollback = false;

    if live_exists {
        ensure_parent_dir(&rollback_path)?;
        fs::rename(&live_path, &rollback_path).with_context(|| {
            format!(
                "Failed to move live path aside during migration finalize: {}",
                live_path.display()
            )
        })?;
        moved_live_to_rollback = true;
    }

    if staged_exists {
        ensure_parent_dir(&live_path)?;
        if let Err(error) = fs::rename(&staged_path, &live_path) {
            if moved_live_to_rollback {
                let _ = fs::rename(&rollback_path, &live_path);
            }
            return Err(error).with_context(|| {
                format!(
                    "Failed to move staged path into place during migration finalize: {}",
                    live_path.display()
                )
            });
        }
    }

    Ok(AppliedPath {
        relative_path: relative_path.to_path_buf(),
        moved_live_to_rollback,
        moved_staged_to_live: staged_exists,
    })
}

fn rollback_applied_paths(
    live_root: &Path,
    rollback_root: &Path,
    applied_paths: &[AppliedPath],
) -> Result<()> {
    for applied_path in applied_paths.iter().rev() {
        let live_path = live_root.join(&applied_path.relative_path);
        let rollback_path = rollback_root.join(&applied_path.relative_path);

        if applied_path.moved_staged_to_live {
            remove_path_if_exists(&live_path)?;
        }
        if applied_path.moved_live_to_rollback {
            ensure_parent_dir(&live_path)?;
            fs::rename(&rollback_path, &live_path).with_context(|| {
                format!(
                    "Failed to restore live path during migration rollback: {}",
                    live_path.display()
                )
            })?;
        }
    }
    Ok(())
}

fn ensure_parent_dir(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create parent directory during migration finalize: {}",
                parent.display()
            )
        })?;
    }
    Ok(())
}

fn remove_path_if_exists(path: &Path) -> Result<()> {
    if !path.exists() {
        return Ok(());
    }

    if path.is_dir() {
        fs::remove_dir_all(path).with_context(|| {
            format!(
                "Failed to remove directory while rolling back migration finalize: {}",
                path.display()
            )
        })?;
    } else {
        fs::remove_file(path).with_context(|| {
            format!(
                "Failed to remove file while rolling back migration finalize: {}",
                path.display()
            )
        })?;
    }
    Ok(())
}

fn normalize_non_empty(value: &str, field_name: &str) -> Result<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        bail!("{field_name} must not be empty");
    }
    Ok(trimmed.to_string())
}

fn normalize_optional(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

fn normalize_fs_path(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            std::path::Component::RootDir => normalized.push(component.as_os_str()),
            std::path::Component::Normal(part) => normalized.push(part),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            std::path::Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
        }
    }
    normalized
}

fn default_legacy_service_state_schema_version() -> u32 {
    LEGACY_SERVICE_STATE_SCHEMA_VERSION
}

#[derive(Debug, Deserialize)]
struct LegacyServiceStateFile {
    #[serde(default = "default_legacy_service_state_schema_version")]
    schema_version: u32,
    #[serde(default)]
    updated_at: String,
    #[serde(default)]
    services: BTreeMap<String, LegacyPersistedService>,
}

#[derive(Debug, Deserialize)]
struct LegacyPersistedService {
    manifest: LegacyServiceManifest,
    desired_state: LegacyDesiredServiceState,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceManifest {
    name: String,
    runtime: LegacyRuntimeKind,
    source: LegacyServiceSource,
    expose: Option<LegacyServiceExpose>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    mounts: Vec<LegacyServiceMount>,
    #[serde(default)]
    ports: Vec<LegacyServicePort>,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    entrypoint: Vec<String>,
    working_dir: Option<String>,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum LegacyRuntimeKind {
    Docker,
    Wasmtime,
}

#[derive(Debug, Clone, Deserialize)]
enum LegacyServiceSource {
    Docker { image: String },
    WasmtimeFile { component: PathBuf },
    WasmtimeUrl { url: String },
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceExpose {
    #[serde(rename = "service_id")]
    _service_id: String,
    #[serde(rename = "display_name")]
    _display_name: String,
    transport: LegacyServiceExposeTransport,
    usage: Option<LegacyServiceExposeUsage>,
    icon_url: Option<String>,
    catalog_id: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceExposeTransport {
    kind: LegacyServiceExposeTransportKind,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum LegacyServiceExposeTransportKind {
    Tcp,
    Raw,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceExposeUsage {
    kind: LegacyServiceExposeUsageKind,
    path: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum LegacyServiceExposeUsageKind {
    Web,
    Ssh,
    Raw,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServiceMount {
    host_path: PathBuf,
    runtime_path: String,
}

#[derive(Debug, Clone, Deserialize)]
struct LegacyServicePort {
    name: Option<String>,
    host_port: u16,
    service_port: u16,
    protocol: LegacyServicePortProtocol,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
enum LegacyServicePortProtocol {
    Tcp,
    Udp,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum LegacyDesiredServiceState {
    Running,
    Stopped,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum CurrentDesiredServiceState {
    Running,
    Stopped,
}

impl From<LegacyDesiredServiceState> for CurrentDesiredServiceState {
    fn from(value: LegacyDesiredServiceState) -> Self {
        match value {
            LegacyDesiredServiceState::Running => Self::Running,
            LegacyDesiredServiceState::Stopped => Self::Stopped,
        }
    }
}

#[derive(Debug, Serialize)]
struct CurrentServiceStateFile {
    schema_version: u32,
    local_service_id: String,
    updated_at: String,
    desired_state: CurrentDesiredServiceState,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestDocument {
    #[serde(rename = "apiVersion")]
    api_version: String,
    kind: String,
    metadata: CurrentServiceManifestMetadata,
    spec: CurrentServiceManifestSpec,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestMetadata {
    name: String,
    #[serde(default)]
    labels: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestSpec {
    runtime: LegacyRuntimeKind,
    source: CurrentServiceManifestSource,
    expose: Option<CurrentServiceManifestExpose>,
    #[serde(default)]
    env: BTreeMap<String, String>,
    #[serde(default)]
    mounts: Vec<CurrentServiceManifestMount>,
    #[serde(default)]
    ports: Vec<CurrentServiceManifestPort>,
    #[serde(default)]
    command: Vec<String>,
    #[serde(default)]
    entrypoint: Vec<String>,
    #[serde(rename = "workingDir")]
    working_dir: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CurrentServiceManifestSource {
    #[serde(skip_serializing_if = "Option::is_none")]
    image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestMount {
    #[serde(rename = "hostPath")]
    host_path: String,
    #[serde(rename = "runtimePath")]
    runtime_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestPort {
    #[serde(rename = "hostPort")]
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    host_port: Option<CurrentServiceManifestHostPort>,
    #[serde(rename = "servicePort")]
    service_port: u16,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    protocol: LegacyServicePortProtocol,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
enum CurrentServiceManifestHostPort {
    Fixed(u16),
    Keyword(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestExpose {
    #[serde(default)]
    enabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    transport: Option<CurrentServiceManifestExposeTransport>,
    #[serde(skip_serializing_if = "Option::is_none")]
    usage: Option<CurrentServiceManifestExposeUsage>,
    #[serde(rename = "iconUrl")]
    #[serde(skip_serializing_if = "Option::is_none")]
    icon_url: Option<String>,
    #[serde(rename = "catalogId")]
    #[serde(skip_serializing_if = "Option::is_none")]
    catalog_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestExposeTransport {
    kind: LegacyServiceExposeTransportKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CurrentServiceManifestExposeUsage {
    kind: LegacyServiceExposeUsageKind,
    path: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use tempfile::TempDir;

    fn fixture_path(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests")
            .join("fixtures")
            .join(name)
    }

    fn read_fixture(name: &str) -> String {
        fs::read_to_string(fixture_path(name)).unwrap()
    }

    #[test]
    fn detects_legacy_no_version_fixture() {
        let detected =
            detect_source_version_from_toml_str(&read_fixture("legacy-no-version-config.toml"))
                .unwrap();

        assert_eq!(detected, DetectedVersion::LegacyNoVersion);
    }

    #[test]
    fn detects_current_version_fixture() {
        let detected =
            detect_source_version_from_toml_str(&read_fixture("current-v2-config.toml")).unwrap();

        assert_eq!(
            detected,
            DetectedVersion::Version(CURRENT_FUNGI_DIR_VERSION)
        );
    }

    #[test]
    fn current_version_is_a_noop() {
        let dir = TempDir::new().unwrap();
        let config_path = dir.path().join(CONFIG_FILE);
        fs::write(&config_path, read_fixture("current-v2-config.toml")).unwrap();

        let report = migrate_if_needed(dir.path()).unwrap();

        assert!(!report.changed);
        assert!(report.backup_dir.is_none());
        assert!(report.staging_dir.is_none());
        assert_eq!(
            report.source_version,
            DetectedVersion::Version(CURRENT_FUNGI_DIR_VERSION)
        );
        assert_eq!(
            fs::read_to_string(config_path).unwrap(),
            read_fixture("current-v2-config.toml")
        );
    }

    #[test]
    fn migrates_legacy_config_transactionally_without_copying_unrelated_side_files() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILE),
            read_fixture("legacy-no-version-config.toml"),
        )
        .unwrap();
        fs::write(
            dir.path().join("devices.toml"),
            "version = \"0.6.1\"\n[devices]\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("access")).unwrap();
        fs::write(
            dir.path().join("access").join("local_access.json"),
            "{\n  \"version\": 1,\n  \"entries\": []\n}\n",
        )
        .unwrap();
        fs::create_dir_all(dir.path().join("cache")).unwrap();
        fs::write(
            dir.path().join("cache").join("direct_addresses.json"),
            "{\n  \"version\": 1,\n  \"addresses\": []\n}\n",
        )
        .unwrap();

        let original_config = fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        let report = migrate_if_needed(dir.path()).unwrap();

        assert!(report.changed);
        assert_eq!(report.source_version, DetectedVersion::LegacyNoVersion);
        assert_eq!(report.target_version, CURRENT_FUNGI_DIR_VERSION);
        assert_eq!(report.migrated_paths, vec![PathBuf::from(CONFIG_FILE)]);

        let migrated_config = fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
        assert!(migrated_config.contains("version = 2"));
        assert_eq!(
            fs::read_to_string(dir.path().join("devices.toml")).unwrap(),
            "version = \"0.6.1\"\n[devices]\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("access").join("local_access.json")).unwrap(),
            "{\n  \"version\": 1,\n  \"entries\": []\n}\n"
        );
        assert_eq!(
            fs::read_to_string(dir.path().join("cache").join("direct_addresses.json")).unwrap(),
            "{\n  \"version\": 1,\n  \"addresses\": []\n}\n"
        );

        let backup_dir = report.backup_dir.expect("backup dir should exist");
        assert!(backup_dir.starts_with(dir.path().join(BACKUP_ROOT_DIR)));
        assert!(
            backup_dir
                .file_name()
                .unwrap()
                .to_string_lossy()
                .contains("legacy-no-version")
        );
        assert_eq!(
            fs::read_to_string(backup_dir.join(CONFIG_FILE)).unwrap(),
            original_config
        );
        assert!(!backup_dir.join("devices.toml").exists());
        assert!(!backup_dir.join("access").exists());
        assert!(!backup_dir.join("cache").exists());

        assert!(report.staging_dir.is_none());
        let staging_count = fs::read_dir(dir.path())
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| {
                entry
                    .file_name()
                    .to_string_lossy()
                    .starts_with(STAGING_DIR_PREFIX)
            })
            .count();
        assert_eq!(staging_count, 0);
    }

    #[test]
    fn migrates_legacy_address_book_to_devices_file() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILE),
            read_fixture("legacy-no-version-config.toml"),
        )
        .unwrap();
        fs::write(
            dir.path().join(LEGACY_ADDRESS_BOOK_FILE),
            concat!(
                "[[peers]]\n",
                "peer_id = \"12D3KooWQW1nMmgC9R2j8uF1pL7dV7cP8xN3d8X7p4hYt3QwD8YV\"\n",
                "alias = \"demo-box\"\n",
                "hostname = \"demo-host\"\n",
                "private_ips = [\"192.168.0.10\"]\n",
                "os = \"MacOS\"\n",
                "version = \"0.6.1\"\n",
                "public_ip = \"203.0.113.10\"\n",
                "created_at = 2026-01-02T03:04:05Z\n",
                "last_connected = 2026-01-03T04:05:06Z\n"
            ),
        )
        .unwrap();

        let report = migrate_if_needed(dir.path()).unwrap();

        assert!(report.changed);
        assert!(dir.path().join(DEVICES_FILE).is_file());
        assert!(!dir.path().join(LEGACY_ADDRESS_BOOK_FILE).exists());

        let devices_value: toml::Value =
            toml::from_str(&fs::read_to_string(dir.path().join(DEVICES_FILE)).unwrap()).unwrap();
        let devices = devices_value
            .as_table()
            .and_then(|table| table.get("devices"))
            .and_then(toml::Value::as_array)
            .unwrap();
        assert_eq!(devices.len(), 1);
        let device = devices[0].as_table().unwrap();
        assert_eq!(
            device.get("name").and_then(toml::Value::as_str),
            Some("demo-box")
        );
        assert_eq!(
            device.get("hostname").and_then(toml::Value::as_str),
            Some("demo-host")
        );
        assert_eq!(
            device
                .get("multiaddrs")
                .and_then(toml::Value::as_array)
                .map(Vec::len),
            Some(0)
        );
        assert_eq!(
            device.get("public_ip").and_then(toml::Value::as_str),
            Some("203.0.113.10")
        );

        let backup_dir = report.backup_dir.expect("backup dir should exist");
        assert!(backup_dir.join(LEGACY_ADDRESS_BOOK_FILE).is_file());
        assert!(!backup_dir.join(".keys").exists());
    }

    #[test]
    fn migrates_legacy_service_state_into_local_service_id_layout_and_moves_service_data() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILE),
            read_fixture("legacy-no-version-config.toml"),
        )
        .unwrap();

        let old_service_dir = dir.path().join(SERVICES_ROOT_DIR).join("demo");
        fs::create_dir_all(old_service_dir.join("cache")).unwrap();
        fs::write(old_service_dir.join("component.wasm"), b"wasm").unwrap();
        fs::write(old_service_dir.join("cache").join("state.txt"), b"persist").unwrap();

        fs::write(
            dir.path().join(LEGACY_SERVICE_STATE_FILE),
            serde_json::to_string_pretty(&json!({
                "schema_version": 1,
                "updated_at": "2026-05-01T00:00:00Z",
                "services": {
                    "demo": {
                        "manifest": {
                            "name": "demo",
                            "runtime": "wasmtime",
                            "source": {
                                "WasmtimeFile": {
                                    "component": old_service_dir.join("component.wasm").display().to_string()
                                }
                            },
                            "expose": {
                                "service_id": "demo-service",
                                "display_name": "Demo Service",
                                "transport": {
                                    "kind": "tcp"
                                },
                                "usage": {
                                    "kind": "web",
                                    "path": "/"
                                },
                                "icon_url": "https://example.com/icon.png",
                                "catalog_id": "demo/catalog"
                            },
                            "env": {},
                            "mounts": [
                                {
                                    "host_path": old_service_dir.join("cache").display().to_string(),
                                    "runtime_path": "/cache"
                                }
                            ],
                            "ports": [
                                {
                                    "name": "http",
                                    "host_port": 18080,
                                    "service_port": 80,
                                    "protocol": "tcp"
                                }
                            ],
                            "command": [],
                            "entrypoint": [],
                            "working_dir": old_service_dir.display().to_string(),
                            "labels": {
                                "demo": "1"
                            }
                        },
                        "desired_state": "stopped"
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();

        let report = migrate_if_needed(dir.path()).unwrap();

        assert!(report.changed);
        assert!(!dir.path().join(LEGACY_SERVICE_STATE_FILE).exists());
        let service_entries = fs::read_dir(dir.path().join(SERVICES_ROOT_DIR))
            .unwrap()
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.file_type().unwrap().is_dir())
            .collect::<Vec<_>>();
        assert_eq!(service_entries.len(), 1);

        let local_service_id = service_entries[0].file_name().to_string_lossy().to_string();
        assert!(local_service_id.starts_with("svc_"));
        assert!(!dir.path().join(SERVICES_ROOT_DIR).join("demo").exists());

        let migrated_data_dir = dir.path().join(DATA_ROOT_DIR).join(&local_service_id);
        assert!(migrated_data_dir.is_dir());
        assert!(migrated_data_dir.join("component.wasm").is_file());
        assert_eq!(
            fs::read_to_string(migrated_data_dir.join("cache").join("state.txt")).unwrap(),
            "persist"
        );

        let manifest_path = dir
            .path()
            .join(SERVICES_ROOT_DIR)
            .join(&local_service_id)
            .join("service.yaml");
        let manifest_yaml = fs::read_to_string(&manifest_path).unwrap();
        assert!(!manifest_yaml.contains("serviceId"));
        assert!(!manifest_yaml.contains("displayName"));
        assert!(!manifest_yaml.contains(&old_service_dir.display().to_string()));
        assert!(manifest_yaml.contains(&migrated_data_dir.display().to_string()));

        let manifest: CurrentServiceManifestDocument =
            serde_yaml::from_str(&manifest_yaml).unwrap();
        assert_eq!(manifest.metadata.name, "demo");
        assert_eq!(
            manifest.spec.working_dir.as_deref(),
            Some(migrated_data_dir.to_string_lossy().as_ref())
        );
        assert_eq!(
            manifest.spec.source.file.as_deref(),
            Some(
                migrated_data_dir
                    .join("component.wasm")
                    .to_string_lossy()
                    .as_ref()
            )
        );
        assert_eq!(manifest.spec.mounts.len(), 1);
        assert_eq!(
            manifest.spec.mounts[0].host_path,
            migrated_data_dir.join("cache").to_string_lossy()
        );

        let state_value: serde_json::Value = serde_json::from_str(
            &fs::read_to_string(
                dir.path()
                    .join(SERVICES_ROOT_DIR)
                    .join(&local_service_id)
                    .join("state.json"),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(
            state_value["schema_version"],
            CURRENT_SERVICE_STATE_SCHEMA_VERSION
        );
        assert_eq!(state_value["local_service_id"], local_service_id);
        assert_eq!(state_value["desired_state"], "stopped");

        let backup_dir = report.backup_dir.expect("backup dir should exist");
        assert!(backup_dir.join(LEGACY_SERVICE_STATE_FILE).is_file());
        assert!(backup_dir.join(SERVICES_ROOT_DIR).join("demo").is_dir());
    }

    #[test]
    fn current_version_with_legacy_artifacts_still_migrates() {
        let dir = TempDir::new().unwrap();
        fs::write(
            dir.path().join(CONFIG_FILE),
            read_fixture("current-v2-config.toml"),
        )
        .unwrap();
        fs::write(dir.path().join(LEGACY_ADDRESS_BOOK_FILE), "peers = []\n").unwrap();
        fs::write(
            dir.path().join(LEGACY_SERVICE_STATE_FILE),
            "{\n  \"schema_version\": 1,\n  \"updated_at\": \"\",\n  \"services\": {}\n}\n",
        )
        .unwrap();

        let report = migrate_if_needed(dir.path()).unwrap();

        assert!(report.changed);
        assert_eq!(
            report.source_version,
            DetectedVersion::Version(CURRENT_FUNGI_DIR_VERSION)
        );
        assert_eq!(report.target_version, CURRENT_FUNGI_DIR_VERSION);
        assert!(dir.path().join(DEVICES_FILE).is_file());
        assert!(!dir.path().join(LEGACY_ADDRESS_BOOK_FILE).exists());
        assert!(!dir.path().join(LEGACY_SERVICE_STATE_FILE).exists());
    }
}
