use crate::{
    CURRENT_FUNGI_DIR_VERSION, DetectedVersion,
    detection::detect_source_version_from_toml_str,
    migrate_if_needed,
    model::{
        BACKUP_ROOT_DIR, CONFIG_FILE, CURRENT_SERVICE_STATE_SCHEMA_VERSION, DATA_ROOT_DIR,
        DEVICES_FILE, LEGACY_ADDRESS_BOOK_FILE, LEGACY_SERVICE_STATE_FILE, SERVICES_ROOT_DIR,
        STAGING_DIR_PREFIX,
    },
};
use serde_json::json;
use std::{fs, path::PathBuf};
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
fn migration_removes_legacy_incoming_allowed_peers_from_config() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join(CONFIG_FILE),
        concat!(
            "incoming_allowed_peers = [\"16Uiu2HAmUSNEhHAAhJsWZU16U8kaBqqXwCPbty8q243amnT2FWe8\"]\n",
            "\n",
            "[rpc]\n",
            "listen_address = \"127.0.0.1:6601\"\n",
        ),
    )
    .unwrap();

    let report = migrate_if_needed(dir.path()).unwrap();

    assert!(report.changed);
    let migrated_config = fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
    assert!(migrated_config.contains("version = 2"));
    assert!(!migrated_config.contains("incoming_allowed_peers"));
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

    let manifest: serde_yaml::Value = serde_yaml::from_str(&manifest_yaml).unwrap();
    assert_eq!(manifest["metadata"]["name"], "demo");
    assert_eq!(
        manifest["spec"]["workingDir"],
        migrated_data_dir.to_string_lossy().as_ref()
    );
    assert_eq!(
        manifest["spec"]["run"]["wasmtime"]["file"],
        migrated_data_dir
            .join("component.wasm")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(manifest["spec"]["entries"]["http"]["port"], 80);
    assert_eq!(manifest["spec"]["entries"]["http"]["usage"], "web");
    assert_eq!(manifest["spec"]["entries"]["http"]["path"], "/");
    let mounts = manifest["spec"]["mounts"].as_sequence().unwrap();
    assert_eq!(mounts.len(), 1);
    assert_eq!(
        mounts[0]["hostPath"],
        migrated_data_dir.join("cache").to_string_lossy().as_ref()
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
