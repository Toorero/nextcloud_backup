use std::{
    collections::HashMap,
    hash::Hash,
    io,
    ops::{Deref, DerefMut},
    path::PathBuf,
    process::Command,
};

use chrono::NaiveDateTime;
use derive_more::{Display, Error};

use super::{SnapperCleanupAlgorithm, SnapperConfig};

/// A snapshot created by snapper.
#[derive(Debug, Clone)]
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

    /// Returns a list of the user data saved in the [Snapshot].
    pub fn user_data<'a>(&'a mut self) -> UserData<'a> {
        UserData { inner: self }
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

#[derive(Debug, Display, Error)]
/// Errors on syncing a [Snapshot].
pub enum SyncSnapshotError {
    /// `btrfs send` failed on syncing.
    #[display("btrfs-send command failed: {_0}")]
    BtrfSendFailed(io::Error),
    #[display("btrfs-receive command failed: {_0}")]
    /// `btrfs receive` failed on syncing.
    BtrfRecvFailed(io::Error),
    /// Couldn't pipe `btrfs send` into `btrfs receive`.
    #[display("pipe between btrfs-send and btrfs-receive failed: {_0}")]
    PipeFailed(io::Error),
    /// Sync destination not found.
    #[display("Sync destination wasn't found: {_0:#?}")]
    DestinationNotFound(#[error(ignore)] PathBuf),
    /// Anchor snapshot wasn't found.
    ///
    /// For [incremental syncing](Snapshot::sync_incrementally) it is required
    /// that the anchor was already synced.
    #[display("Anchor snapshot isn't synced: {_0:?}")]
    AnchorNotSynced(#[error(ignore)] Snapshot),
}
