use std::{
    collections::HashMap,
    io::{self, BufRead, BufReader},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
};

use chrono::NaiveDateTime;
use derive_more::{Display, Error};
use log::Level;

use super::{SnapperCleanupAlgorithm, SnapperConfig};

/// Snapper userdata key to identify the incremental sync anchor.
const ANCHOR_ID: &str = "anchor";
/// Snapper userdata key to identify already synched snapshots.
pub(super) const SYNCED_ID: &str = "synced";

#[derive(Debug, Clone)]
pub struct Snapshot {
    config: SnapperConfig,
    id: u64,
    user_data: HashMap<String, String>,
    cleanup: Option<SnapperCleanupAlgorithm>,
    date: NaiveDateTime,
}

impl PartialEq for Snapshot {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl Snapshot {
    pub(super) fn new(
        config: SnapperConfig,
        id: u64,
        user_data: HashMap<String, String>,
        cleanup: Option<SnapperCleanupAlgorithm>,
        date: NaiveDateTime,
    ) -> Self {
        Self {
            config,
            id,
            user_data,
            cleanup,
            date,
        }
    }
}

// read snapshot data
impl Snapshot {
    pub fn user_data(&self) -> &HashMap<String, String> {
        &self.user_data
    }

    pub(super) fn is_anchored(&self) -> bool {
        self.user_data.get(ANCHOR_ID).is_some_and(|d| d == "true")
    }

    pub(super) fn is_synced(&self) -> bool {
        self.user_data.get(SYNCED_ID).is_some_and(|d| d == "true")
    }

    pub(super) fn is_unsynced(&self) -> bool {
        self.user_data.get(SYNCED_ID).is_some_and(|d| d == "false")
    }

    pub(super) fn id(&self) -> u64 {
        self.id
    }

    pub fn date(&self) -> &NaiveDateTime {
        &self.date
    }

    fn snapshot_path(&self) -> PathBuf {
        self.config
            .subvolume
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

        let snapper_output = Command::new("snapper")
            .arg("--jsonout")
            .arg("-c")
            .arg(&self.config.config_id)
            .arg("modify")
            .arg("-u")
            .arg(user_data)
            .arg("-c")
            .arg(cleanup)
            .arg(self.id.to_string())
            .output()
            .expect("Failed to execute snapper command");

        log::trace!(target: "backend::snapper::snapshot", "Updated snapshot meta data: {:?}", self);
        assert!(snapper_output.status.success());
    }

    pub fn set_cleanup(&mut self, cleanup_algorithm: Option<SnapperCleanupAlgorithm>) {
        self.cleanup = cleanup_algorithm;
        self.update();
    }

    pub(super) fn anchor(&mut self) {
        self.user_data
            .insert(ANCHOR_ID.to_string(), "true".to_string());
        self.update();
    }

    pub(super) fn release(&mut self) {
        // HACK: don't delete becase deletion of keys is not updated
        self.user_data
            .insert(ANCHOR_ID.to_string(), "false".to_string());
        self.update();
    }

    fn synced(&mut self) {
        self.user_data
            .insert(SYNCED_ID.to_string(), "true".to_string());
        self.update();
    }

    // TODO: Allow others update user data using RAII
}

// sync methods
impl Snapshot {
    pub fn sync(&mut self, sync_destination: &Path) -> Result<(), SyncSnapshotError> {
        log::info!(target: "backend::snapper", "Syncing snapshot in full: {:?}", self);

        self.sync_maybe_incrementally(None, sync_destination)?;

        log::trace!(target: "backend::snapper", "Syncing of snapshot completed: {:?}", self);
        Ok(())
    }

    pub fn sync_incrementally(
        &mut self,
        anchor: &Snapshot,
        sync_destination: &Path,
    ) -> Result<(), SyncSnapshotError> {
        log::debug!(target: "backend::snapper:snapshot", "Syncing snapshot incrementally: {:?} ({:?}) -> {}", self, anchor, sync_destination.display());

        self.sync_maybe_incrementally(Some(anchor), sync_destination)?;

        log::trace!(target: "backend::snapper", "Syncing of snapshot completed: {:?}", self);

        Ok(())
    }

    fn sync_maybe_incrementally(
        &mut self,
        anchor: Option<&Snapshot>,
        sync_destination: &Path,
    ) -> Result<(), SyncSnapshotError> {
        let snapshot_path = self.snapshot_path();
        assert!(snapshot_path.is_dir(), "snapshot must exist");
        assert!(sync_destination.exists(), "sync destination must exist");

        // TODO: support compressed sending?
        // WARNING: Sending/Receiving snapshots sadly requires root permissions/sudo
        //          add the following (or similar line) into your sudoers:
        //          `www-data ALL=(ALL:ALL) NOPASSWD: /usr/bin/btrfs`
        let mut btrfs_send = Command::new("sudo");
        btrfs_send.arg("btrfs");
        // enable verbose btrfs-send output
        if log::log_enabled!(target: "backend::snapper::snapshot::btrfs-send", Level::Trace) {
            btrfs_send.arg("-v");
        }
        btrfs_send.arg("send");

        // BTRFS-SEND
        // add parent snapshot argument if sending incrementally
        if let Some(anchor) = anchor {
            let anchor_path = anchor.snapshot_path();
            assert!(anchor_path.is_dir(), "path of anchor snapshot must exist");
            assert!(anchor.is_synced(), "anchor should have been synced already");

            btrfs_send.arg("-p").arg(anchor_path);
        }
        let mut btrfs_send = btrfs_send
            .arg(snapshot_path)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(SyncSnapshotError::BtrfSendFailed)?;
        log::trace!(target: "backend::snapper::snapshot", "started btrfs-send: {:?}", self);

        // log btrfs send output
        let btrfs_send_log = if log::log_enabled!(target: "backend::snapper::snapshot::btrfs-send", Level::Trace)
        {
            let stderr = btrfs_send.stderr.take().unwrap();
            Some(thread::spawn(move || {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();

                while let Some(Ok(line)) = lines.next() {
                    log::trace!(target: "backend::snapper::snapshot::btrfs-send", "{}", line);
                }
                log::trace!(target: "backend::snapper::snapshot::btrfs-send", "SEND RELAY COMPLETED");
            }))
        } else {
            None
        };

        // BTRFS-RECEIVE
        let mut btrfs_recv = Command::new("sudo");
        btrfs_recv.arg("btrfs");
        // enable verbose btrfs-receive output
        if log::log_enabled!(target: "backend::snapper::snapshot::btrfs-receive", Level::Trace) {
            btrfs_recv.arg("-v");
        }
        btrfs_recv.arg("receive");

        let mut btrfs_recv = btrfs_recv
            .arg(sync_destination)
            .stdin(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(SyncSnapshotError::BtrfRecvFailed)?;
        log::trace!(target: "backend::snapper::snapshot", "started btrfs-receive: {:?}", self);

        // log btrfs recv output
        let btrfs_recv_log = if log::log_enabled!(target: "backend::snapper::snapshot::btrfs-receive", Level::Trace)
        {
            let stderr = btrfs_recv.stderr.take().unwrap();
            Some(thread::spawn(move || {
                let reader = BufReader::new(stderr);
                let mut lines = reader.lines();

                while let Some(Ok(line)) = lines.next() {
                    log::trace!(target: "backend::snapper::snapshot::btrfs-receive", "{}", line);
                }
                log::trace!(target: "backend::snapper::snapshot::btrfs-receive", "RECEIVE RELAY COMPLETED");
            }))
        } else {
            None
        };

        // PIPE
        let mut stdout = btrfs_send.stdout.take().unwrap();
        let mut stdin = btrfs_recv.stdin.take().unwrap();
        io::copy(&mut stdout, &mut stdin).map_err(SyncSnapshotError::PipeFailed)?;

        // signal completion of btrfs-send to btrfs-receive by closing stdin
        drop(stdin);
        drop(stdout);

        // WAIT for completion

        assert!(btrfs_send_log
            .map(|handle| handle.join().is_ok())
            .unwrap_or(true));
        {
            let status = btrfs_send
                .wait()
                .map_err(SyncSnapshotError::BtrfSendFailed)?;
            if !status.success() {
                let err = io::Error::new(
                    io::ErrorKind::Other,
                    format!("btrfs send failed with status {}", status),
                );
                let btrf_send_failed = SyncSnapshotError::BtrfSendFailed(err);
                return Err(btrf_send_failed);
            }
            log::trace!(target: "backend::snapper::snapshot", "btrf-send complete: {:?}", self);
        }

        assert!(btrfs_recv_log
            .map(|handle| handle.join().is_ok())
            .unwrap_or(true));
        {
            let status = btrfs_recv
                .wait()
                .map_err(SyncSnapshotError::BtrfRecvFailed)?;
            if !status.success() {
                let err = io::Error::new(
                    io::ErrorKind::Other,
                    format!("btrfs receive failed with status {}", status),
                );
                let btrf_recv_failed = SyncSnapshotError::BtrfRecvFailed(err);
                return Err(btrf_recv_failed);
            }
            log::trace!(target: "backend::snapper::snapshot", "btrf-receive complete: {:?}", self);
        }

        self.synced();
        assert!(self.is_synced());
        Ok(())
    }
}

#[derive(Debug, Display, Error)]
pub enum SyncSnapshotError {
    #[display("btrfs-send command failed")]
    BtrfSendFailed(io::Error),
    #[display("btrfs-receive command failed")]
    BtrfRecvFailed(io::Error),
    #[display("pipe failed")]
    PipeFailed(io::Error),
}
