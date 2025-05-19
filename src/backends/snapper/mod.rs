use std::str::FromStr;
use std::{io, path::PathBuf};

use clap::ValueEnum;
use derive_more::{Display, Error};

use crate::backends::Backup;
use config::SnapperConfig;

pub mod config;
pub mod snapshot;

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
    /// This backend utilizes [`btrfs-send(8)`] and [`btrfs-receive(8)`]
    /// to send snapshots incrementally.
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
}

impl Snapper {}

#[derive(Debug, Display, Error)]
pub enum SnapperError {
    #[display("Snapper config not found for {_0}")]
    SnapperConfigNotFound(#[error(ignore)] String),
    #[display("Unable to create sync destination folder")]
    SyncDestinationCantBeCreated(io::Error),
}

impl Backup for Snapper {
    type Error = SnapperError;

    fn backup(
        &mut self,
        nextcloud: &crate::nextcloud::Nextcloud,
        _dry_run: bool, // TODO: support dry_run
    ) -> Result<(), Self::Error> {
        let data_dir = nextcloud.occ.data_directory();
        assert!(data_dir.is_dir(), "Nextcloud Data directory should exist");

        let cfg = SnapperConfig::by_dir(&data_dir).ok_or(SnapperError::SnapperConfigNotFound(
            format!("{}", data_dir.display()),
        ))?;

        // TODO: sync deletion

        // mark snapshot not synced
        let _ = cfg.create_snapshot(self.cleanup_algorithm);

        let Some(ref sync_destination) = self.sync_destination else {
            log::warn!(target: "backend::snapper", "Not syncing snapshots to other destination");
            return Ok(());
        };

        let mut orig_anchor = cfg.anchored_snapshot();
        let mut anchor = orig_anchor.clone();
        if let Some(ref mut anchor) = anchor {
            log::debug!(target: "backend::snapper", "Found anchor snapshot of last sync: {:?}", anchor);
        }

        // WARN: maybe we need to sort them a smart way?
        // in theory there should only be one unsynced snapshot
        for mut snap in cfg.unsynced_snapshots() {
            let sync_destination = sync_destination.join(format!("{}/", snap.id()));
            std::fs::create_dir_all(&sync_destination)
                .map_err(SnapperError::SyncDestinationCantBeCreated)?;

            if let Some(ref mut anchor) = anchor {
                // sync snapshot incrementally using our anchor snapshot
                snap.sync_incrementally(anchor, &sync_destination).unwrap();

                // update anchor to newly synced snapshot
                *anchor = snap;
                log::trace!(target: "backend::snapper", "Promoted snapshot to new anchor: {:?}", anchor);
            } else {
                // sync initial snapshot so we can later sync incrementally
                snap.sync(&sync_destination).unwrap();

                // promote to anchor
                anchor = Some(snap);
                log::trace!(target: "backend::snapper", "Promoted snapshot to new anchor: {:?}", anchor.as_ref().unwrap());
            }
        }

        let mut anchor = anchor.expect("after syncing there has to be an anchor");
        log::debug!(target: "backend::snapper", "Anchoring snapshot for next time: {:?}", anchor);
        anchor.anchor();
        anchor.set_cleanup(None); // prevent deletion before next sync/backup
        let anchor = anchor;

        if let Some(ref mut orig_anchor) = orig_anchor {
            assert_ne!(&anchor, orig_anchor, "anchor should change after syncing");

            log::debug!(target: "backend::snapper", "Releasing previous anchor snapshot: {:?}", orig_anchor);
            orig_anchor.release();
            orig_anchor.set_cleanup(self.cleanup_algorithm); // restore cleanup algorithm because this anchor is now no longer needed
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
