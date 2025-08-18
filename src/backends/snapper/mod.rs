//! Implements backup of Nextcloud's data using [Snapper].

use std::collections::HashSet;
use std::ffi::OsStr;
use std::process::{Command, Stdio};
use std::str::FromStr;
use std::{io, path::PathBuf};

use clap::ValueEnum;
use derive_more::{Display, Error, From};
use log::Level;

use crate::nextcloud::{Nextcloud, OccError};
use super::Backup;


mod config;
mod snapshot;

pub use snapshot::{Snapshot, SyncSnapshotError};
pub use config::{SnapperConfig, SnapperConfigError};

/// [Snapper](http://snapper.io): A backend utilizing the btrfs snapshot capabilities.
///
/// It's possible to additionally send snapshots to different locations
/// for redundancy. See [`sync_desetionation`](Self::sync_destination) for more details.
#[derive(Debug)]
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

    /// Snapshots created by [Snapper] can be send to a different location
    /// to have the data stored at multiple locations.
    /// This backend utilizes [`btrfs-send(8)`] and [`btrfs-receive(8)`].
    ///
    /// <div class="warning">
    /// The deletion of snapshots is synced to the destination as well.
    /// </div>
    ///
    /// This backend guarantees that at least one backup by this backend
    /// is present to allow redundant transfers to happen incrementally.
    ///
    /// [`btrfs-send(8)`]: https://man.archlinux.org/man/core/btrfs-progs/btrfs-send.8.en
    /// [`btrfs-receive(8)`]: https://man.archlinux.org/man/core/btrfs-progs/btrfs-receive.8.en
    pub sync_destination: Option<PathBuf>,

    /// If set snapshots are send incrementally using [`btrfs-send(8)`] and [`btrfs-receive(8)`].
    /// Otherwise all snapshots are synced in full utilizing the same method.
    ///
    /// [`btrfs-send(8)`]: https://man.archlinux.org/man/core/btrfs-progs/btrfs-send.8.en
    /// [`btrfs-receive(8)`]: https://man.archlinux.org/man/core/btrfs-progs/btrfs-receive.8.en
    pub incrementally: bool,
}

impl Snapper {}

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
    /// Obtaining the anchor [Snapshot] failed.
    #[display("Obtaining anchor snapshot failed: {_0}")]
    ObtainingAnchorFailed(SnapperConfigError),
    /// Obtaining unsynced [Snapshot] failed.
    #[display("Obtaining unsynced snapshot(s) failed: {_0}")]
    ObtainingUnsyncedFailed(SnapperConfigError),
    

    /// Nextcloud `occ` command failed.
    #[from]
    Occ(OccError),
}

impl Backup for Snapper {
    type Error = SnapperBackupError;

    fn backup(
        &mut self,
        nextcloud: &Nextcloud,
        _dry_run: bool, // TODO: support dry_run
    ) -> Result<(), Self::Error> {
        let data_dir = nextcloud.occ().data_directory()?;
        assert!(data_dir.is_dir(), "Nextcloud Data directory should exist");

        let cfg = SnapperConfig::by_dir(&data_dir).map_err(SnapperBackupError::SnapperConfig)?
            .ok_or(SnapperBackupError::SnapperConfigNotFound(data_dir))?;

        let _ = cfg.create_snapshot(self.cleanup_algorithm).map_err(SnapperBackupError::CreationFailed)?;

        let Some(ref sync_destination) = self.sync_destination else {
            log::warn!(target: "backend::snapper", "Not syncing snapshots to other destination");
            return Ok(());
        };

        // delete subvolumes at sync destination that are not present at source
        match sync_destination.read_dir() {
            Ok(synced) => {
                log::debug!(target: "backend::snapper", "Synchronize deletion to sync destination");

                let present_snapshots = match cfg.snapshots() {
                    Ok(present_snapshots) => present_snapshots.iter().map(Snapshot::id).collect(),
                    Err(e) => {
                        log::warn!(target: "backend::snapper", "Can't determine present snapshots: {e}");
                        HashSet::with_capacity(0)
                    }
                };
                log::trace!(target: "backend::snapper", "Snapshots present: {present_snapshots:?}");

                let subv_deletions = synced.filter_map(|entry| {
                    let entry = entry.ok()?;
                    let path = entry.path();
                    if !path.is_dir() {
                        return None;
                    }
                    
                    // delete empty dirs
                    if std::fs::remove_dir(&path).is_ok() {
                        log::trace!(target: "backend::snapper", "Deleted empty direcotry at sync destination: {}", path.display());
                        return None;
                    }

                    if !path.join("snapshot/").is_dir() {
                        return None;
                    }
                    let Some(Ok(snapshot_id)): Option<Result<u64, _>> =
                        path.file_name().and_then(OsStr::to_str).map(str::parse)
                    else {
                        return None;
                    };
                    log::trace!(target: "backend::snapper", "Found snapshot present at sync destination: {snapshot_id}");

                    // don't delete present snapshots!
                    if present_snapshots.contains(&snapshot_id) {
                        return None;
                    }

                    log::debug!(target: "backend::snapper", "Sync deletion of snapshot to sync destination: {snapshot_id}");
                    let mut btrfs_subv_del = Command::new("sudo");
                    btrfs_subv_del.arg("btrfs");
                    // enable verbose btrfs-receive output
                    if log::log_enabled!(target: "backend::snapper", Level::Trace) {
                        btrfs_subv_del.arg("-v");

                        log::trace!(
                            target: "backend::snapper",
                            "Running: sudo btrfs -v subvolume delete {}",
                            path.join("snapshot/").display()
                        );
                    }
                    btrfs_subv_del.arg("subvolume").arg("delete");

                    let btrfs_subv_del = btrfs_subv_del
                        .arg(path.join("snapshot/"))
                        .stdout(Stdio::null())
                        .stderr(Stdio::null())
                        .spawn();
                    
                    if let Err(ref e) = btrfs_subv_del {
                        log::error!(target: "backend::snapper", "Deletion of snapshot {snapshot_id} at sync destination failed: {e}");
                    }
                    btrfs_subv_del.ok().map(|c| (c, snapshot_id))
                });

                // wait for completion of all deletions
                for (mut deletion, snapshot_id) in subv_deletions {
                    match deletion.wait() {
                        Ok(status) if status.success() => {
                             log::trace!(target: "backend::snapper", "Finished deletion of snapshot at sync destination: {snapshot_id}");
                        }
                        Ok(status) => {
                             log::error!(target: "backend::snapper", "Deletion of snapshot {snapshot_id} at sync destination failed: {status}");
                        }
                        Err(e) => log::error!(target: "backend::snapper", "Couldn't run deletion of snapshot {snapshot_id} at sync destination: {e}"),
                    }
                }
            }
            Err(_) => {
                std::fs::create_dir_all(sync_destination)
                    .map_err(SnapperBackupError::SyncDestinationCantBeCreated)?;
            }
        }

        let mut orig_anchor = cfg.anchored_snapshot().map_err(SnapperBackupError::ObtainingAnchorFailed)?;
        let mut anchor = orig_anchor.clone();
        if let Some(ref anchor) = anchor {
            log::debug!(target: "backend::snapper", "Found anchor snapshot of last sync: {anchor:?}");
        }

        // WARN: maybe we need to sort them a smart way?
        // in theory there should only be one unsynced snapshot
        for mut snap in cfg.unsynced_snapshots().map_err(SnapperBackupError::ObtainingUnsyncedFailed)? {
            let sync_destination = sync_destination.join(format!("{}/", snap.id()));

            if let Some(ref mut anchor) = anchor {
                // sync snapshot incrementally using our anchor snapshot
                if self.incrementally {
                    snap.sync_incrementally(anchor, &sync_destination).unwrap();
                } else {
                    snap.sync(&sync_destination).unwrap();
                }

                // update anchor to newly synced snapshot
                *anchor = snap;
                log::trace!(target: "backend::snapper", "Promoted snapshot to new anchor: {anchor:?}");
            } else {
                // sync initial snapshot so we can later sync incrementally
                snap.sync(&sync_destination).unwrap();

                // promote to anchor
                anchor = Some(snap);
                log::trace!(target: "backend::snapper", "Promoted snapshot to new anchor: {:?}", anchor.as_ref().unwrap());
            }
        }

        let mut anchor = anchor.expect("after syncing there has to be an anchor");
        log::debug!(target: "backend::snapper", "Anchoring snapshot for next time: {anchor:?}");
        anchor.anchor();
        anchor.set_cleanup(None); // prevent deletion before next sync/backup
        let anchor = anchor;

        if let Some(ref mut orig_anchor) = orig_anchor {
            assert_ne!(&anchor, orig_anchor, "anchor should change after syncing");

            log::debug!(target: "backend::snapper", "Releasing previous anchor snapshot: {orig_anchor:?}");
            orig_anchor.release();
            // restore cleanup algorithm because this anchor is now no longer needed
            orig_anchor.set_cleanup(self.cleanup_algorithm);
        }

        Ok(())
    }
}

/// Algorithms provided by Snapper to clean up old snapshots.
///
/// The algorithms are executed in a daily cronjob or systemd timer.
/// This can be configured in the corresponding snapper configurations files
/// along with parameters for every algorithm.
#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, ValueEnum, Debug, Display)]
pub enum SnapperCleanupAlgorithm {
    /// Deletes old snapshots when a certain number of snapshots is reached.
    #[display("number")]
    Number,
    /// Deletes old snapshots but keeps a number of hourly, daily, weekly, monthly and yearly snapshots.
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
