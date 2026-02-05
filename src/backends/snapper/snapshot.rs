use std::{
    collections::HashMap,
    hash::Hash,
    ops::{Deref, DerefMut},
    path::PathBuf,
    process::Command,
};

use chrono::NaiveDateTime;

use crate::backends::snapper::SnapperConfigError;

use super::{SnapperCleanupAlgorithm, SnapperConfig};

/// A snapshot created by snapper.
#[derive(Debug)]
pub struct Snapshot {
    config: SnapperConfig,
    id: u64,
    user_data: HashMap<String, String>,
    cleanup: Option<SnapperCleanupAlgorithm>,
    date: NaiveDateTime,
    description: Option<String>,
}

impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}
impl Eq for Snapshot {}

impl Hash for Snapshot {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl Snapshot {
    pub(super) fn new(
        config: SnapperConfig,
        id: u64,
        user_data: HashMap<String, String>,
        cleanup: Option<SnapperCleanupAlgorithm>,
        date: NaiveDateTime,
        description: Option<String>,
    ) -> Self {
        Self {
            config,
            id,
            user_data,
            cleanup,
            date,
            description,
        }
    }
}

impl Snapshot {
    pub(super) fn id(&self) -> u64 {
        self.id
    }

    /// Creation date of the snapshot.
    pub fn date(&self) -> &NaiveDateTime {
        &self.date
    }

    /// Path to the snapshot.
    fn snapshot_path(&self) -> PathBuf {
        self.config
            .subvolume()
            .join(format!(".snapshots/{}/snapshot", self.id))
    }
}

// snapshot manipulation
impl Snapshot {
    fn update(&mut self) {
        // FIXME: cover deletion of keys
        let user_data = self
            .user_data()
            .iter()
            .map(|(k, v)| format!("{k}={v}"))
            .collect::<Vec<_>>()
            .join(",");
        let cleanup = self.cleanup.map(|c| c.to_string()).unwrap_or_default();

        if let Some(description) = &self.description {
            log::trace!(
                target: "backend::snapper::snapshot",
                "Running: snapper --jsonout -c {} modify -u {user_data} -c {cleanup} -d {description} {}",
                self.config.config_id(),
                self.id
            );
        } else {
            log::trace!(
                target: "backend::snapper::snapshot",
                "Running: snapper --jsonout -c {} modify -u {user_data} -c {cleanup} {}",
                self.config.config_id(),
                self.id
            );
        }

        let mut snapper_cmd = Command::new("snapper");
        snapper_cmd
            .arg("--jsonout")
            .arg("-c")
            .arg(&self.config.config_id)
            .arg("modify")
            .arg("-u")
            .arg(user_data)
            .arg("-c")
            .arg(cleanup)
            .arg(self.id.to_string());

        if let Some(description) = &self.description {
            snapper_cmd.arg("-d").arg(description);
        }

        let snapper_output = snapper_cmd
            .output()
            .expect("Failed to execute snapper command");

        log::debug!(target: "backend::snapper::snapshot", "Updated snapshot meta data: {self:?}");
        assert!(snapper_output.status.success());
    }

    /// Set the cleanup algorithm.
    pub fn set_cleanup(&mut self, cleanup_algorithm: Option<SnapperCleanupAlgorithm>) {
        self.cleanup = cleanup_algorithm;
        self.update();
    }

    /// Set the description.
    pub fn set_description(&mut self, description: String) {
        self.description = Some(description);
        self.update();
    }

    /// Returns a map of the user data saved in the [Snapshot].
    pub fn user_data(&self) -> &HashMap<String, String> {
        &self.user_data
    }

    /// Returns a mutable map of the user data saved in the [Snapshot].
    pub fn user_data_mut<'a>(&'a mut self) -> UserData<'a> {
        UserData { inner: self }
    }

    pub fn delete(self) -> Result<(), SnapperConfigError> {
        self.delete_maybe_dry_run(false)
    }
    pub fn delete_dry_run(self) -> Result<(), SnapperConfigError> {
        self.delete_maybe_dry_run(true)
    }

    fn delete_maybe_dry_run(self, dry_run: bool) -> Result<(), SnapperConfigError> {
        let mut snapper_command = Command::new("snapper");
        snapper_command
            .arg("-c")
            .arg(&self.config.config_id)
            .arg("delete")
            .arg(format!("{}", self.id));

        log::trace!(
            target: "backends::snapper::config",
            "Running: snapper -c {} remove {}",
            self.id,
            self.config.config_id,
        );
        if dry_run {
            return Ok(());
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

        let stderr = String::from_utf8_lossy(&snapper_output.stderr);

        if !stderr.is_empty() {
            log::warn!(target: "backend::snapper", "{stderr}" );
        }
        Ok(())
    }
}

pub struct UserData<'a> {
    inner: &'a mut Snapshot,
}

impl<'a> Deref for UserData<'a> {
    type Target = HashMap<String, String>;

    fn deref(&self) -> &Self::Target {
        &self.inner.user_data
    }
}

impl<'a> DerefMut for UserData<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner.user_data
    }
}

impl<'a> Drop for UserData<'a> {
    fn drop(&mut self) {
        self.inner.update()
    }
}
