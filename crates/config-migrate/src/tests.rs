use crate::{
    CURRENT_FUNGI_DIR_VERSION, DetectedVersion,
    apply::MigrationTransaction,
    detection::detect_source_version_from_toml_str,
    migrate_if_needed,
    model::{
        BACKUP_ROOT_DIR, CONFIG_FILE, CURRENT_SERVICE_STATE_SCHEMA_VERSION, DEVICES_FILE,
        LEGACY_ADDRESS_BOOK_FILE, LEGACY_SERVICE_STATE_FILE, SERVICES_ROOT_DIR, STAGING_DIR_PREFIX,
        TRUSTED_DEVICES_FILE,
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

#[test]
fn migration_transaction_prepare_allocates_fresh_backup_dirs() {
    let dir = TempDir::new().unwrap();

    let first =
        MigrationTransaction::prepare(dir.path(), &DetectedVersion::LegacyNoVersion, 7).unwrap();
    let second =
        MigrationTransaction::prepare(dir.path(), &DetectedVersion::LegacyNoVersion, 7).unwrap();

    assert_ne!(first.backup_dir, second.backup_dir);
    assert_ne!(first.staging_dir, second.staging_dir);
    assert!(first.backup_dir.is_dir());
    assert!(second.backup_dir.is_dir());
    assert!(first.staging_dir.is_dir());
    assert!(second.staging_dir.is_dir());
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
        detect_source_version_from_toml_str(&read_fixture("current-v3-config.toml")).unwrap();

    assert_eq!(
        detected,
        DetectedVersion::Version(CURRENT_FUNGI_DIR_VERSION)
    );
}

#[test]
fn migrates_version_two_to_final_version_three_schema() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join(CONFIG_FILE),
        concat!(
            "version = 2\n",
            "\n",
            "[network]\n",
            "relay_enabled = true\n",
        ),
    )
    .unwrap();

    let report = migrate_if_needed(dir.path()).unwrap();

    assert!(report.changed);
    assert_eq!(report.source_version, DetectedVersion::Version(2));
    assert_eq!(report.target_version, 3);
    assert!(
        fs::read_to_string(dir.path().join(CONFIG_FILE))
            .unwrap()
            .contains("version = 3")
    );
}

#[test]
fn current_version_is_a_noop() {
    let dir = TempDir::new().unwrap();
    let config_path = dir.path().join(CONFIG_FILE);
    fs::write(&config_path, read_fixture("current-v3-config.toml")).unwrap();

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
        read_fixture("current-v3-config.toml")
    );
}

#[test]
fn migrates_legacy_config_transactionally_and_keeps_full_backup_snapshot() {
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
    fs::create_dir_all(dir.path().join("cache")).unwrap();
    fs::write(
        dir.path().join("cache").join("local_preferences.json"),
        "[]\n",
    )
    .unwrap();
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
    assert!(migrated_config.contains("version = 3"));
    assert_eq!(
        fs::read_to_string(dir.path().join("devices.toml")).unwrap(),
        "version = \"0.6.1\"\n[devices]\n"
    );
    assert_eq!(
        fs::read_to_string(dir.path().join("cache").join("local_preferences.json")).unwrap(),
        "[]\n"
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
    assert_eq!(
        fs::read_to_string(backup_dir.join("devices.toml")).unwrap(),
        "version = \"0.6.1\"\n[devices]\n"
    );
    assert_eq!(
        fs::read_to_string(backup_dir.join("cache").join("local_preferences.json")).unwrap(),
        "[]\n"
    );
    assert_eq!(
        fs::read_to_string(backup_dir.join("cache").join("direct_addresses.json")).unwrap(),
        "{\n  \"version\": 1,\n  \"addresses\": []\n}\n"
    );
    assert!(!backup_dir.join(BACKUP_ROOT_DIR).exists());

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
    let root_peer = "12D3KooWQW1nMmgC9R2j8uF1pL7dV7cP8xN3d8X7p4hYt3QwD8YV";
    let network_peer = "12D3KooWJGXfQ3X6E3FJxMeYpPvVhDhxDBBkYDpAJrYaHsxkUTdT";
    fs::write(
        dir.path().join(CONFIG_FILE),
        format!(
            "incoming_allowed_peers = [\"{root_peer}\"]\n\
             \n\
             [rpc]\n\
             listen_address = \"127.0.0.1:6601\"\n\
             \n\
             [network]\n\
             incoming_allowed_peers = [\"{network_peer}\", \"{root_peer}\"]\n",
        ),
    )
    .unwrap();

    let report = migrate_if_needed(dir.path()).unwrap();

    assert!(report.changed);
    let migrated_config = fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
    assert!(migrated_config.contains("version = 3"));
    assert!(!migrated_config.contains("incoming_allowed_peers"));

    let trusted_value: toml::Value =
        toml::from_str(&fs::read_to_string(dir.path().join(TRUSTED_DEVICES_FILE)).unwrap())
            .unwrap();
    let trusted_devices = trusted_value
        .as_table()
        .and_then(|table| table.get("trusted_devices"))
        .and_then(toml::Value::as_array)
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(trusted_devices, vec![network_peer, root_peer]);
}

#[test]
fn migration_merges_legacy_incoming_allowed_peers_into_existing_trusted_devices_file() {
    let dir = TempDir::new().unwrap();
    let existing_peer = "12D3KooWQW1nMmgC9R2j8uF1pL7dV7cP8xN3d8X7p4hYt3QwD8YV";
    let new_peer = "12D3KooWJGXfQ3X6E3FJxMeYpPvVhDhxDBBkYDpAJrYaHsxkUTdT";
    fs::write(
        dir.path().join(CONFIG_FILE),
        format!(
            "version = 2\n\
             \n\
             [network]\n\
             incoming_allowed_peers = [\"{existing_peer}\", \"{new_peer}\"]\n",
        ),
    )
    .unwrap();
    fs::write(
        dir.path().join(TRUSTED_DEVICES_FILE),
        format!("trusted_devices = [\"{existing_peer}\"]\n"),
    )
    .unwrap();

    let report = migrate_if_needed(dir.path()).unwrap();

    assert!(report.changed);
    let trusted_value: toml::Value =
        toml::from_str(&fs::read_to_string(dir.path().join(TRUSTED_DEVICES_FILE)).unwrap())
            .unwrap();
    let trusted_devices = trusted_value
        .as_table()
        .and_then(|table| table.get("trusted_devices"))
        .and_then(toml::Value::as_array)
        .unwrap()
        .iter()
        .map(|value| value.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(trusted_devices, vec![new_peer, existing_peer]);
}

#[test]
fn migration_removes_config_sections_that_no_longer_exist() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join(CONFIG_FILE),
        concat!(
            "[rpc]\n",
            "listen_address = \"127.0.0.1:6601\"\n",
            "\n",
            "[network]\n",
            "listen_tcp_port = 4101\n",
            "listen_udp_port = 4102\n",
            "relay_enabled = false\n",
            "\n",
            "[tcp_tunneling.forwarding]\n",
            "enabled = true\n",
            "rules = []\n",
            "\n",
            "[file_transfer]\n",
            "client = []\n",
            "\n",
            "[runtime]\n",
            "disable_docker = true\n",
            "disable_wasmtime = false\n",
            "allowed_ports = [18080]\n",
            "\n",
            "[[runtime.allowed_port_ranges]]\n",
            "start = 19000\n",
            "end = 19100\n",
        ),
    )
    .unwrap();

    migrate_if_needed(dir.path()).unwrap();

    let migrated = fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
    assert!(!migrated.contains("tcp_tunneling"));
    assert!(!migrated.contains("file_transfer"));
    assert!(!migrated.contains("allowed_ports"));
    assert!(!migrated.contains("allowed_port_ranges"));
    assert!(migrated.contains("listen_address = \"127.0.0.1:6601\""));
    assert!(migrated.contains("listen_tcp_port = 4101"));
    assert!(migrated.contains("listen_udp_port = 4102"));
    assert!(migrated.contains("relay_enabled = false"));
    assert!(migrated.contains("disable_docker = true"));
}

#[test]
fn version_two_with_removed_config_sections_is_normalized() {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join(CONFIG_FILE),
        concat!(
            "version = 2\n",
            "\n",
            "[network]\n",
            "relay_enabled = true\n",
            "\n",
            "[tcp_tunneling.forwarding]\n",
            "enabled = true\n",
            "rules = []\n",
        ),
    )
    .unwrap();

    let report = migrate_if_needed(dir.path()).unwrap();

    assert!(report.changed);
    let migrated = fs::read_to_string(dir.path().join(CONFIG_FILE)).unwrap();
    assert!(!migrated.contains("tcp_tunneling"));
    assert!(migrated.contains("version = 3"));
}

#[test]
fn migrates_legacy_local_access_records_to_local_preferences() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(CONFIG_FILE), "version = 2\n").unwrap();
    fs::create_dir_all(dir.path().join("access")).unwrap();
    fs::write(
        dir.path().join("access").join("local_access.json"),
        serde_json::to_vec_pretty(&json!({
            "records": [{
                "remote_peer_id": "peer-a",
                "remote_service_name": "fb",
                "remote_service_port_name": "http",
                "local_host": "127.0.0.1",
                "local_port": 8082,
                "local_port_source": "user",
                "last_remote_protocol": "/fungi/service-port/fb/http/0.1.0",
                "last_remote_port": 8080
            }]
        }))
        .unwrap(),
    )
    .unwrap();

    let report = migrate_if_needed(dir.path()).unwrap();

    assert!(report.changed);
    assert!(!dir.path().join("access").join("local_access.json").exists());
    let preferences: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(dir.path().join("cache").join("local_preferences.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(preferences.as_array().unwrap().len(), 1);
    assert_eq!(preferences[0]["remote_peer_id"], "peer-a");
    assert_eq!(preferences[0]["remote_service_name"], "fb");
    assert_eq!(preferences[0]["remote_service_port_name"], "http");
    assert_eq!(preferences[0]["local_host"], "127.0.0.1");
    assert_eq!(preferences[0]["local_port"], 8082);
    assert_eq!(preferences[0]["local_port_source"], "user");
    assert!(preferences[0].get("last_remote_protocol").is_none());
    assert!(preferences[0].get("last_remote_port").is_none());
}

#[test]
fn removes_obsolete_remote_service_caches_after_backing_them_up() {
    let dir = TempDir::new().unwrap();
    fs::write(dir.path().join(CONFIG_FILE), "version = 2\n").unwrap();
    let published = dir.path().join("cache").join("remote_services");
    let managed = dir.path().join("cache").join("device_managed_services");
    fs::create_dir_all(&published).unwrap();
    fs::create_dir_all(&managed).unwrap();
    fs::write(published.join("peer-a.json"), "published").unwrap();
    fs::write(managed.join("peer-a.json"), "managed").unwrap();

    let report = migrate_if_needed(dir.path()).unwrap();

    assert!(report.changed);
    assert!(!published.exists());
    assert!(!managed.exists());
    let backup = report.backup_dir.unwrap();
    assert_eq!(
        fs::read_to_string(backup.join("cache/remote_services/peer-a.json")).unwrap(),
        "published"
    );
    assert_eq!(
        fs::read_to_string(backup.join("cache/device_managed_services/peer-a.json")).unwrap(),
        "managed"
    );
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

    let migrated_appdata_dir = dir
        .path()
        .join("appdata")
        .join("services")
        .join(&local_service_id);
    let migrated_artifacts_dir = dir
        .path()
        .join("artifacts")
        .join("services")
        .join(&local_service_id);
    assert!(migrated_appdata_dir.is_dir());
    assert!(migrated_artifacts_dir.join("component.wasm").is_file());
    assert!(!migrated_appdata_dir.join("component.wasm").exists());
    assert_eq!(
        fs::read_to_string(migrated_appdata_dir.join("cache").join("state.txt")).unwrap(),
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
    assert!(!manifest_yaml.contains("workingDir"));
    assert!(!manifest_yaml.contains("labels"));

    let manifest: serde_yaml::Value = serde_yaml::from_str(&manifest_yaml).unwrap();
    assert_eq!(manifest["fungi"], "service/v1");
    assert_eq!(manifest["id"], "demo");
    assert_eq!(manifest["run"]["provider"], "wasmtime");
    assert_eq!(manifest["run"]["mode"], "http");
    assert_eq!(
        manifest["run"]["source"]["file"],
        "$fungi.service.artifacts/component.wasm"
    );
    assert_eq!(manifest["publish"]["http"]["tcp"]["port"], 18080);
    assert_eq!(manifest["publish"]["http"]["client"]["kind"], "web");
    assert_eq!(manifest["publish"]["http"]["client"]["path"], "/");
    assert_eq!(
        manifest["publish"]["http"]["client"]["iconUrl"],
        "https://example.com/icon.png"
    );
    let mounts = manifest["run"]["mounts"].as_sequence().unwrap();
    assert_eq!(mounts.len(), 1);
    assert_eq!(mounts[0]["from"], "$fungi.service.data/cache");
    assert_eq!(mounts[0]["to"], "/cache");

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
        read_fixture("current-v3-config.toml"),
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
