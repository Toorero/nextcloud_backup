use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::NaiveDateTime;
use serde_json::Value;

use super::snapshot::{Snapshot, SYNCED_ID};
use super::SnapperCleanupAlgorithm;

#[derive(Debug, Clone)]
pub struct SnapperConfig {
    pub subvolume: PathBuf,
    pub config_id: String,
}

impl PartialEq for SnapperConfig {
    fn eq(&self, other: &Self) -> bool {
        self.config_id == other.config_id
    }
}

impl SnapperConfig {
    pub fn by_dir(dir: &Path) -> Option<SnapperConfig> {
        let snapper_output = Command::new("snapper")
            .arg("--jsonout")
            .arg("list-configs")
            .output()
            .expect("Failed to execute snapper command");
        assert!(snapper_output.status.success(), "snapper command failed");

        let stderr = String::from_utf8_lossy(&snapper_output.stderr);
        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{}", stderr );
        }

        let jsonout: Value =
            serde_json::from_slice(&snapper_output.stdout).expect("json should be valid");
        let configs = jsonout
            .get("configs")
            .expect("command should return a list of configs")
            .as_array()
            .expect("json list of configs should be an array");

        configs.iter().find_map(|config| {
            let config_id = config.get("config").and_then(Value::as_str)?;
            let subvolume = PathBuf::from(config.get("subvolume").and_then(Value::as_str)?);

            if subvolume == dir {
                Some(Self {
                    config_id: config_id.to_string(),
                    subvolume,
                })
            } else {
                None
            }
        })
    }

    pub fn config_by_id(config_id: &str) -> Option<SnapperConfig> {
        let snapper_output = Command::new("snapper")
            .arg("--jsonout")
            .arg("-c")
            .arg(config_id)
            .arg("get-config")
            .output()
            .expect("Failed to execute snapper command");

        if !snapper_output.status.success() {
            log::warn!(target: "backend::snapper::config", "Snapper configuration unknown: {config_id}");
            return None;
        }

        let stderr = String::from_utf8_lossy(&snapper_output.stderr);
        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{}", stderr );
        }

        let jsonout: Value =
            serde_json::from_slice(&snapper_output.stdout).expect("json should be valid");

        let subvolume = PathBuf::from(jsonout.get("SUBVOLUME").and_then(Value::as_str)?);
        let config_id = config_id.to_string();

        Some(Self {
            config_id,
            subvolume,
        })
    }
}

impl SnapperConfig {
    pub fn snapshots(&self) -> Vec<Snapshot> {
        let snapper_output = Command::new("snapper")
            .arg("--jsonout")
            .arg("-c")
            .arg(&self.config_id)
            .arg("list")
            .arg("--columns")
            .arg("number,userdata,cleanup,date")
            .output()
            .expect("Failed to execute snapper command");
        assert!(snapper_output.status.success(), "snapper command failed");

        let stderr = String::from_utf8_lossy(&snapper_output.stderr);

        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{}", stderr );
        }

        let jsonout: Value =
            serde_json::from_slice(&snapper_output.stdout).expect("json should be valid");

        let snapshots = jsonout
            .get(&self.config_id)
            .expect("command should return snapshots matching the supplied configuration")
            .as_array()
            .expect("json snapshot list should be an array");

        snapshots
            .iter()
            .filter_map(|snapshot| {
                let snap_id = snapshot.get("number").and_then(|v| v.as_u64())?;

                let userdata = snapshot
                    .get("userdata")
                    .and_then(|v| v.as_object())
                    .map(|map| {
                        map.into_iter()
                            .filter_map(|(k, v)| {
                                let v = v.as_str()?;
                                Some((k.clone(), v.to_string()))
                            })
                            .collect()
                    })
                    .unwrap_or_default();

                let cleanup = snapshot
                    .get("cleanup")
                    .and_then(|v| v.as_str())
                    .and_then(|s| s.parse().ok());

                let date = snapshot
                    .get("date")
                    .and_then(|v| v.as_str())
                    .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())?;

                let snapshot = Snapshot::new(self.clone(), snap_id, userdata, cleanup, date);
                Some(snapshot)
            })
            .collect()
    }

    pub fn snapshot(&self, snapshot_id: u64) -> Option<Snapshot> {
        self.snapshots()
            .into_iter()
            .find(|snap| snap.id() == snapshot_id)
    }

    pub fn unsynced_snapshots(&self) -> impl Iterator<Item = Snapshot> {
        self.snapshots().into_iter().filter(Snapshot::is_unsynced)
    }

    pub fn anchored_snapshot(&self) -> Option<Snapshot> {
        debug_assert_eq!(
            self.snapshots()
                .into_iter()
                .filter(Snapshot::is_anchored)
                .skip(1)
                .next(),
            None,
            "there should only be one anchor"
        );

        self.snapshots().into_iter().find(Snapshot::is_anchored)
    }

    pub fn create_snapshot(&self, cleanup: Option<SnapperCleanupAlgorithm>) -> Snapshot {
        log::debug!(target: "backends::snapper::config", "Create snapshot: {}", self.config_id);

        let mut snapper_command = Command::new("snapper");
        snapper_command
            .arg("-c")
            .arg(&self.config_id)
            .arg("create")
            .arg("-p") // echo snapshot id
            .arg("-u")
            .arg(format!("{SYNCED_ID}=false"))
            .arg("--description")
            .arg("Full Nextcloud Backup");

        if let Some(algorithm) = cleanup {
            snapper_command.arg("-c");
            snapper_command.arg(algorithm.to_string());
        }

        let snapper_output = snapper_command
            .output()
            .expect("Failed to execute snapper command");
        assert!(snapper_output.status.success(), "snapper command failed");

        let stdout = String::from_utf8_lossy(&snapper_output.stdout);
        let stderr = String::from_utf8_lossy(&snapper_output.stderr);

        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{}", stderr );
        }

        let id = stdout
            .trim()
            .parse()
            .expect("snapper should output valid snapshot id");
        log::trace!(target: "backends::snapper::config", "Created snapshot: {}", id);

        self.snapshot(id)
            .expect("just created snapshot should exist")
    }
}
