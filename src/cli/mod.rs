use std::{path::PathBuf, str::FromStr};

use clap::{ArgAction, Args, Parser, Subcommand};
use log::LevelFilter;

use crate::backends::snapper::{SnapperCleanupAlgorithm, UnkownCleanupAlgorithm};

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Verbosity of the command output.
    #[arg(long)]
    pub verbose: Option<LevelFilter>,

    /// Root directory of the Nextcloud server instance.
    #[arg(long, default_value = "/var/www/nextcloud")]
    pub document_root: PathBuf,

    /// Update nextcloud apps after backup.
    #[arg(long)]
    pub update: bool,

    /// Nextcloud backup notification receiver account.
    #[arg(long, default_value = "admin")]
    pub admin: String,

    /// Send summery notification to the admin Nextcloud account.
    #[arg(
        long = "no-notification",
        action=ArgAction::SetFalse
    )]
    pub notification: bool,

    /// Simulative backup run.
    #[arg(long)]
    pub dry_run: bool,

    /// Prefix of the log file.
    #[arg(long, default_value = "log-")]
    pub log_prefix: String,

    /// Days of log files to keep.
    #[arg(long, default_value = "3")]
    pub log_days: u8,

    /// Days of Nextcloud config and  database to keep.
    #[arg(long, default_value = "35")]
    pub backup_days: u8,

    #[arg(long, short = 'r')]
    /// Folder for Nextcloud config and database backups and backup-logs.
    pub backup_root: PathBuf,

    /// A backend utilizing the btrfs snapshot capabilities. See: http://snapper.io
    #[arg(long, group = "data_backend", default_value = "true")]
    pub snapper: bool,

    #[command(flatten)]
    pub snapper_args: SnapperArgs,

    //#[arg(long, group = "data_backend")]
    //pub rsync: bool,
    #[command(subcommand)]
    pub action: Option<Action>,
}

#[derive(Args, Debug)]
#[group(multiple = true, requires = "snapper")]
pub struct SnapperArgs {
    /// Destination on where to sync snapper snapshots to.
    #[arg(long = "sync-dest", short = 'd')]
    pub sync_destination: Option<PathBuf>,

    /// Algorithm to later clean up created snapshots.
    #[arg(long = "cleanup-algorithm", short = 'c', default_value = "timeline")]
    pub cleanup: MaybeSnapperCleanupAlgorithm,
}

// HACK: Clap has "issues" with utilizing a ValueParser for Option<SnapperCleanupAlgorithm>...
#[derive(Debug, Clone)]
pub enum MaybeSnapperCleanupAlgorithm {
    None,
    SnapperCleanupAlgorithm(SnapperCleanupAlgorithm),
}

impl FromStr for MaybeSnapperCleanupAlgorithm {
    type Err = UnkownCleanupAlgorithm;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.trim().is_empty() {
            Ok(Self::None)
        } else {
            let cleanup = SnapperCleanupAlgorithm::from_str(s)?;
            Ok(Self::SnapperCleanupAlgorithm(cleanup))
        }
    }
}

impl From<MaybeSnapperCleanupAlgorithm> for Option<SnapperCleanupAlgorithm> {
    fn from(value: MaybeSnapperCleanupAlgorithm) -> Self {
        match value {
            MaybeSnapperCleanupAlgorithm::None => None,
            MaybeSnapperCleanupAlgorithm::SnapperCleanupAlgorithm(cleanup) => Some(cleanup),
        }
    }
}

#[derive(Subcommand, Debug, Default)]
pub enum Action {
    /// Backup the Nextcloud config, database and data. (Default)
    #[default]
    Backup,
}
