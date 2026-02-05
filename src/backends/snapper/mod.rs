//! Implements backup of Nextcloud's data using [Snapper].

use std::str::FromStr;
use std::{io, path::PathBuf};

use clap::ValueEnum;
use derive_more::{Display, Error, From};

use super::Backup;
use crate::backends::snapper::config::SNAPPER_USERDATA_TAG;
use crate::nextcloud::{Nextcloud, OccError};
use crate::util::retention::{Retention, RetentionConfig};

mod config;
mod snapshot;

pub use config::{SnapperConfig, SnapperConfigError};
pub use snapshot::Snapshot;

/// [Snapper](http://snapper.io): A backend utilizing the btrfs snapshot capabilities.
///
/// It's possible to additionally send snapshots to different locations
/// for redundancy. See [`sync_desetionation`](Self::sync_destination) for more details.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct Snapper {
    /// Algorithms to clean up old snapshots.
    ///
    /// Cleanups are made by *independently* of this backend by snapper itself.
    /// For information on how to configure [Snapper] to perform periodic cleanups
    /// consult [`snapper(8)`]
    ///
    /// <div class="warning">
    /// Clean up algorithms don't distinguish between snapshots created
    /// by this tool or by snapper itself.
    /// </div>
    ///
    /// [`snapper(8)`]: https://man.archlinux.org/man/snapper.8
    pub cleanup_algorithm: Option<SnapperCleanupAlgorithm>,
}

impl Default for Snapper {
    fn default() -> Self {
        Self {
            cleanup_algorithm: Some(Default::default()),
        }
    }
}

#[derive(Debug, Display, Error, From)]
/// Errors on backup of the data directory of the [Nextcloud] installation.
pub enum SnapperBackupError {
    /// No Snapper config for the data directory of [Nextcloud] found.
    #[display("Snapper config not found")]
    SnapperConfigNotFound(#[error(ignore)] PathBuf),
    /// Sync destination can't be created.
    #[display("Unable to create sync destination folder")]
    SyncDestinationCantBeCreated(io::Error),
    /// Obtaining the [SnapperConfig] of the [Nextcloud] installation failed.
    #[display("Obtaining the snapper-config of the nextcloud installation failed: {_0}")]
    SnapperConfig(SnapperConfigError),
    /// Creating a [Snapshot] as backup failed.
    #[display("Creating a backup snapshot failed: {_0}")]
    CreationFailed(SnapperConfigError),
    /// Listing [Snapshot] failed.
    #[display("Listing snapshots failed: {_0}")]
    ListSnapshotsFailed(SnapperConfigError),

    /// Nextcloud `occ` command failed.
    #[from]
    Occ(OccError),
}

impl Backup for Snapper {
    type Error = SnapperBackupError;

    fn backup(&self, nextcloud: &Nextcloud, dry_run: bool) -> Result<(), Self::Error> {
        let data_dir = nextcloud.occ().data_directory()?;
        assert!(data_dir.is_dir(), "Nextcloud Data directory should exist");

        let cfg = SnapperConfig::by_dir(&data_dir)
            .map_err(SnapperBackupError::SnapperConfig)?
            .ok_or(SnapperBackupError::SnapperConfigNotFound(data_dir))?;

        if dry_run {
            cfg.create_snapshot_dry_run(self.cleanup_algorithm)
                .map_err(SnapperBackupError::CreationFailed)?;
        } else {
            let _snapshot = cfg
                .create_snapshot(self.cleanup_algorithm)
                .map_err(SnapperBackupError::CreationFailed)?;
        }

        Ok(())
    }

    fn retention(
        &self,
        nextcloud: &Nextcloud,
        retention_cfg: &RetentionConfig,
        dry_run: bool,
    ) -> Result<(), Self::Error> {
        let data_dir = nextcloud.occ().data_directory()?;
        let cfg = SnapperConfig::by_dir(&data_dir)
            .map_err(SnapperBackupError::SnapperConfig)?
            .ok_or(SnapperBackupError::SnapperConfigNotFound(data_dir))?;

        let mut snapshots: Vec<_> = cfg
            .snapshots()
            .map_err(SnapperBackupError::ListSnapshotsFailed)?
            .into_iter()
            .filter(|s| s.user_data().contains_key(SNAPPER_USERDATA_TAG)) // only manage snapshots created by the this program
            .collect();
        // keep the most recent backups of each kind
        snapshots.sort_by(|s1, s2| s1.date().cmp(s2.date()).reverse());

        let mut retention = Retention::from(*retention_cfg);
        for snapshot in snapshots {
            if retention.retain(*snapshot.date()) {
                log::debug!(target: "backend::config::retain", "Snapshot retained: {}", snapshot.id());
                continue;
            }

            log::info!(target: "backend::config::retain", "Discarding snapshot: {}", snapshot.id());
            if dry_run {
                if let Err(e) = snapshot.delete_dry_run() {
                    log::error!(target: "backend::config::retain", "Error deleting snapshot: {e}");
                }
            } else if let Err(e) = snapshot.delete() {
                log::error!(target: "backend::config::retain", "Error deleting snapshot: {e}");
            }
        }

        Ok(())
    }
}

/// Algorithms provided by Snapper to clean up old snapshots.
///
/// The algorithms are executed in a daily cronjob or systemd timer.
/// This can be configured in the corresponding snapper configurations files
/// along with parameters for every algorithm.
#[derive(Copy, Clone, ValueEnum, Debug, Display, Default, serde::Serialize, serde::Deserialize)]
pub enum SnapperCleanupAlgorithm {
    /// Deletes old snapshots when a certain number of snapshots is reached.
    #[display("number")]
    Number,
    /// Deletes old snapshots but keeps a number of hourly, daily, weekly, monthly and yearly snapshots.
    #[default]
    #[display("timeline")]
    Timeline,
}

/// Cleanup algorithm set by [Snapper] is unknown.
#[derive(Debug, Display, Error)]
#[display("Cleanup algorithm is unkown: {_0}")]
pub struct UnkownCleanupAlgorithm(#[error(ignore)] String);

impl FromStr for SnapperCleanupAlgorithm {
    type Err = UnkownCleanupAlgorithm;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "number" => Ok(Self::Number),
            "timeline" => Ok(Self::Timeline),
            other => Err(UnkownCleanupAlgorithm(other.to_string())),
        }
    }
}
