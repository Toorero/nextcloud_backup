//! Components for the binary command-line interface.

use std::path::PathBuf;

use clap::{ArgAction, Args, Parser, Subcommand, ValueEnum};
use log::LevelFilter;

use crate::nextcloud::DEFAULT_INSTALLATION_ROOT;

/// Main command-line struct.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Cli {
    /// Verbosity of the command output.
    #[arg(short, long)]
    pub verbose: Option<LevelFilter>,

    /// Directory of the Nextcloud server installation.
    #[arg(short = 'd', long, default_value = DEFAULT_INSTALLATION_ROOT)]
    pub document_root: PathBuf,

    #[arg(long, short = 'r')]
    /// Root folder used by backup modules to put their data into.
    pub backup_root: PathBuf,

    /// Nextcloud notification receiver account.
    #[arg(long, default_value = "admin")]
    pub admin: String,
    /// Send summery notifications to the admin Nextcloud account.
    #[arg(
        long = "no-notification",
        action=ArgAction::SetFalse
    )]
    pub notification: bool,

    #[arg(short, long, default_value = "/etc/nc_backup.toml")]
    /// Path to `nc_backup.toml`
    pub config: PathBuf,

    /// List of enabled backends.
    #[arg(
        short = 'b',
        long,
        value_delimiter = ',',
        default_value = "config,maria-db,snapper"
    )]
    pub enabled_backends: Vec<Backends>,

    /// Simulative run which doesn't alter any files.
    #[arg(long)]
    pub dry_run: bool,

    /// Actions to perform.
    #[command(subcommand)]
    pub action: Action,
}

#[derive(Debug, ValueEnum, Clone, Hash, PartialEq, Eq)]
/// Available backends.
pub enum Backends {
    /// Backup of Nextcloud's `config.php`.
    Config,
    /// Backup of Nextcloud's mariadb.
    MariaDb,
    /// Incremental backup of Nextcloud's data using Snapper.
    ///
    /// Requires external setup.
    Snapper,
}

#[derive(Debug, Clone, Subcommand)]
/// Action to perform.
pub enum Action {
    /// Backup the Nextcloud config, database and data.
    Backup(BackupArgs),
    /// Retain backups.
    Retain,
}

#[derive(Debug, Args, Default, Clone)]
/// Arguments to tune the backup of the Nextcloud instance.
pub struct BackupArgs {
    /// Update nextcloud apps after backup.
    #[arg(long)]
    pub update: bool,
}
