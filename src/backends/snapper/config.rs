use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;

use chrono::NaiveDateTime;
use derive_more::{Display, Error};
use serde_json::Value;

use super::snapshot::Snapshot;
use super::SnapperCleanupAlgorithm;

pub(super) const SNAPPER_USERDATA_TAG: &str = "nc_backup";

#[derive(Debug, Clone)]
/// A configuration of snapper.
pub struct SnapperConfig {
    pub(super) subvolume: PathBuf,
    pub(super) config_id: String,
}

impl PartialEq for SnapperConfig {
    fn eq(&self, other: &Self) -> bool {
        self.config_id == other.config_id
    }
}

#[derive(Debug, Display, Error)]
/// Error of [SnapperConfig].
pub enum SnapperConfigError {
    /// `snapper` command could not be run.
    ///
    /// This is usually the case if `snapper` isn't installed locally.
    #[display("Snapper command couldn't be run: {_0}")]
    SnapperNotRun(io::Error),
    /// Snapper command failed.
    #[display("Snapper command {command:?} failed with error: {error}")]
    SnapperCommandFailed {
        /// [Command] that failed.
        #[error(ignore)]
        command: Box<Command>,
        /// Captured stderr.
        #[error(ignore)]
        error: String,
    },
}

type Result<T> = std::result::Result<T, SnapperConfigError>;

impl SnapperConfig {
    /// Create a new [SnapperConfig].
    pub fn new(subvolume: PathBuf, config_id: String) -> Result<Self> {
        log::trace!(
            target: "backends::snapper::config",
            "Running: snapper -c {config_id} create-config {subvolume:#?}"
        );

        let mut snapper_command = Command::new("snapper");
        snapper_command
            .arg("-c")
            .arg(&config_id)
            .arg("create-config")
            .arg(subvolume.as_os_str());
        let snapper_output = snapper_command
            .output()
            .map_err(SnapperConfigError::SnapperNotRun)?;
        let stderr = String::from_utf8_lossy(&snapper_output.stderr);
        if !snapper_output.status.success() {
            return Err(SnapperConfigError::SnapperCommandFailed {
                command: Box::new(snapper_command),
                error: stderr.into(),
            });
        }
        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{stderr}" );
        }

        Ok(SnapperConfig {
            subvolume,
            config_id,
        })
    }

    /// Find an *existing* snapper config by directory.
    pub fn by_dir(dir: &Path) -> Result<Option<SnapperConfig>> {
        log::trace!(
            target: "backends::snapper::config",
            "Running: snapper --jsonout list-configs"
        );
        let mut snapper_command = Command::new("snapper");
        snapper_command.arg("--jsonout").arg("list-configs");
        let snapper_output = snapper_command
            .output()
            .map_err(SnapperConfigError::SnapperNotRun)?;
        let stderr = String::from_utf8_lossy(&snapper_output.stderr);
        if !snapper_output.status.success() {
            return Err(SnapperConfigError::SnapperCommandFailed {
                command: Box::new(snapper_command),
                error: stderr.into(),
            });
        }
        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{stderr}" );
        }

        let jsonout: Value = serde_json::from_slice(&snapper_output.stdout)
            .expect("snapper json output should be valid");
        let configs = jsonout
            .get("configs")
            .expect("command should return a list of configs")
            .as_array()
            .expect("json list of configs should be an array");

        Ok(configs.iter().find_map(|config| {
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
        }))
    }

    /// Find an *existing* [SnapperConfig] by its config-id.
    pub fn config_by_id(config_id: &str) -> Result<Option<SnapperConfig>> {
        log::trace!(
            target: "backends::snapper::config",
            "Running: snapper --jsonout -c {config_id} get-config"
        );
        let mut snapper_command = Command::new("snapper");
        snapper_command
            .arg("--jsonout")
            .arg("-c")
            .arg(config_id)
            .arg("get-config");
        let snapper_output = snapper_command
            .output()
            .map_err(SnapperConfigError::SnapperNotRun)?;
        let stderr = String::from_utf8_lossy(&snapper_output.stderr);
        if !snapper_output.status.success() {
            return Err(SnapperConfigError::SnapperCommandFailed {
                command: Box::new(snapper_command),
                error: stderr.into(),
            });
        }
        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{stderr}" );
        }

        let stderr = String::from_utf8_lossy(&snapper_output.stderr);
        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{stderr}" );
        }

        let jsonout: Value = serde_json::from_slice(&snapper_output.stdout)
            .expect("snapper json output should be valid");
        let Some(subvolume) = jsonout.get("SUBVOLUME").and_then(Value::as_str) else {
            return Ok(None);
        };
        let subvolume = PathBuf::from(subvolume);
        let config_id = config_id.to_string();

        Ok(Some(Self {
            config_id,
            subvolume,
        }))
    }

    /// The subvolume that is managed by the [SnapperConfig].
    pub fn subvolume(&self) -> PathBuf {
        self.subvolume.clone()
    }

    /// The config id of the [SnapperConfig].
    pub fn config_id(&self) -> &str {
        &self.config_id
    }
}

impl SnapperConfig {
    /// List all snapshots associated with the [SnapperConfig].
    pub fn snapshots(&self) -> Result<Vec<Snapshot>> {
        log::trace!(
            target: "backends::snapper::config",
            "Running: snapper --jsonout -c {} list --columns number,userdata,cleanup,date,description",
            self.config_id
        );
        let mut snapper_command = Command::new("snapper");
        snapper_command
            .arg("--jsonout")
            .arg("-c")
            .arg(&self.config_id)
            .arg("list")
            .arg("--columns")
            .arg("number,userdata,cleanup,date,description");
        let snapper_output = snapper_command
            .output()
            .map_err(SnapperConfigError::SnapperNotRun)?;
        let stderr = String::from_utf8_lossy(&snapper_output.stderr);
        if !snapper_output.status.success() {
            return Err(SnapperConfigError::SnapperCommandFailed {
                command: Box::new(snapper_command),
                error: stderr.into(),
            });
        }
        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{stderr}" );
        }

        let jsonout: Value = serde_json::from_slice(&snapper_output.stdout)
            .expect("snapper json output should be valid");

        let snapshots = jsonout
            .get(&self.config_id)
            .expect("command should return snapshots matching the supplied configuration")
            .as_array()
            .expect("json snapshot list should be an array");

        Ok(snapshots
            .iter()
            .filter_map(|snapshot| {
                let snap_id = snapshot.get("number").and_then(|v| v.as_u64())?;

                let userdata = snapshot
                    .get("userdata")
                    .and_then(serde_json::Value::as_object)
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
                    .and_then(serde_json::Value::as_str)
                    .and_then(|s| NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").ok())?;

                let description = snapshot
                    .get("description")
                    .and_then(serde_json::Value::as_str)
                    .map(String::from);

                let snapshot =
                    Snapshot::new(self.clone(), snap_id, userdata, cleanup, date, description);
                Some(snapshot)
            })
            .collect())
    }

    /// Return snapshot with `snapshot_id` if present.
    pub fn snapshot(&self, snapshot_id: u64) -> Result<Option<Snapshot>> {
        Ok(self
            .snapshots()?
            .into_iter()
            .find(|snap| snap.id() == snapshot_id))
    }

    /// Create a new snapshot.
    ///
    /// If no [SnapperCleanupAlgorithm] is provided the snapshot must be manually deleted later.
    pub fn create_snapshot(&self, cleanup: Option<SnapperCleanupAlgorithm>) -> Result<Snapshot> {
        Ok(self
            .create_snapshot_maybe_dry_run(cleanup, false)?
            .expect("non dry run should create snapshot on success"))
    }

    pub fn create_snapshot_dry_run(&self, cleanup: Option<SnapperCleanupAlgorithm>) -> Result<()> {
        let res = self.create_snapshot_maybe_dry_run(cleanup, true)?;
        assert_eq!(res, None, "dry run should not create snapshot on success");
        Ok(())
    }

    pub fn create_snapshot_maybe_dry_run(
        &self,
        cleanup: Option<SnapperCleanupAlgorithm>,
        dry_run: bool,
    ) -> Result<Option<Snapshot>> {
        log::info!(target: "backends::snapper::config", "Create snapshot: {}", self.config_id);

        let mut snapper_command = Command::new("snapper");
        snapper_command
            .arg("-c")
            .arg(&self.config_id)
            .arg("create")
            .arg("-p") // echo snapshot id
            .arg("--userdata")
            .arg(format!("{SNAPPER_USERDATA_TAG}=true"))
            .arg("--description")
            .arg("Full Nextcloud Backup");

        if let Some(algorithm) = cleanup {
            snapper_command.arg("-c");
            snapper_command.arg(algorithm.to_string());

            log::trace!(
                target: "backends::snapper::config",
                "Running: snapper -c {} create -p  --userdata {SNAPPER_USERDATA_TAG}=true --description 'Full Nextcloud Backup' -c {algorithm}",
                self.config_id,
            );
        } else {
            log::trace!(
                target: "backends::snapper::config",
                "Running: snapper -c {} create -p --userdata {SNAPPER_USERDATA_TAG}=true --description 'Full Nextcloud Backup'",
                self.config_id,
            );
        }
        if dry_run {
            return Ok(None);
        }

        let snapper_output = snapper_command
            .output()
            .map_err(SnapperConfigError::SnapperNotRun)?;
        let stderr = String::from_utf8_lossy(&snapper_output.stderr);
        if !snapper_output.status.success() {
            return Err(SnapperConfigError::SnapperCommandFailed {
                command: Box::new(snapper_command),
                error: stderr.into(),
            });
        }
        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{stderr}" );
        }

        let stdout = String::from_utf8_lossy(&snapper_output.stdout);
        let stderr = String::from_utf8_lossy(&snapper_output.stderr);

        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{stderr}" );
        }

        let id = stdout
            .trim()
            .parse()
            .expect("snapper should output valid snapshot id");
        log::info!(target: "backends::snapper::config", "Created snapshot: {id}");

        Ok(Some(
            self.snapshot(id)?
                .expect("just created snapshot should exist"),
        ))
    }
}
