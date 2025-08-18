//! Implements backup of Nextcloud's mariadb using [MariaDb].

use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus, Stdio};

use chrono::Local;
use derive_more::{Display, Error, From};
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::backends::Backup;
use crate::nextcloud::{Nextcloud, OccError};

const DB_DUMP_DEST: &str = "db/";

/// Allows you to backup the
pub struct MariaDb {
    db_dump_dest: PathBuf,
}

impl MariaDb {
    /// Create a new [MariaDb] instance.
    pub fn new(backup_root: &Path) -> Self {
        let db_dump_dest = backup_root.join(DB_DUMP_DEST);
        if db_dump_dest.is_relative() {
            log::warn!(target: "backend::mariadb", "db_dump_dest is relative: {}", db_dump_dest.display());
        }

        Self { db_dump_dest }
    }

    fn generate_db_dump_filename(&self) -> PathBuf {
        let timestamp = Local::now().format("%Y-%m-%dT%H-%M-%S");

        let path = self
            .db_dump_dest
            .join(format!("database-{timestamp}.sql.gz"));
        assert!(!path.exists(), "db dump file should not exist prior");

        path
    }
}

#[derive(Debug, Display, Error, From)]
/// Error on backup of the database.
pub enum MariaDbError {
    /// Failed to dump the database.
    #[display("mariadb-dump failed with {_0}")]
    DumpFailed(#[error(ignore)] ExitStatus),
    /// Failed to spawn the `mariadb-dump` process.
    ///
    /// Usually this is caused by not having `mariadb-dump` installed.
    #[display("Failed to spawn mariadb-dump: {_0}")]
    MariaDbDump(io::Error),
    /// Destination of the dump already exists.
    ///
    /// To save you from potential data loss the backup won't overwrite old backups.
    #[display("Dump destination already exists: {_0}")]
    DestinationExists(io::Error),

    /// Error on running an `occ` command.
    #[from]
    Occ(OccError),
    /// Generic [io::Error].
    ///
    /// Usually the cause is that dump can't be written to the destination.
    #[from]
    Io(io::Error),
}

impl Backup for MariaDb {
    type Error = MariaDbError;

    fn backup(&mut self, nextcloud: &Nextcloud, dry_run: bool) -> Result<(), Self::Error> {
        let table_name = nextcloud.occ().db_name()?;
        let table_usr = nextcloud.occ().db_user()?;
        log::info!(target: "backend::mariadb", "Create database dump of the Nextcloud table: {table_name}");
        log::debug!(target: "backend::mariadb", "Using dbuser '{table_usr}' for backup");

        fs::create_dir_all(&self.db_dump_dest)?;
        let db_dump_file = self.generate_db_dump_filename();
        log::debug!(target: "backend::mariadb", "Save Nextcloud database dump at: {}", db_dump_file.display());

        log::trace!(
            target: "backend::mariadb",
            "Running: mariadb-dump --opt --single-transaction --user={table_usr} {table_name}"
        );
        let mut dump_process = Command::new("mariadb-dump")
            .arg("--opt") // sensible dump defaults
            .arg("--single-transaction")
            .arg(format!("--user={table_usr}"))
            .arg(table_name)
            .stdout(Stdio::piped())
            .spawn()
            .map_err(MariaDbError::MariaDbDump)?;
        log::trace!(target: "backend::mariadb", "Started mariadb-dump process.");

        // compress and capture stdout of mariadb-dump
        let stdout = dump_process
            .stdout
            .take()
            .expect("stdout should be untaken");
        let mut reader = BufReader::new(stdout);
        if dry_run {
            log::trace!(target: "backend::mariadb", "Discarding output of mariadb-dump on dry-run");
            let mut sink = io::sink();
            std::io::copy(&mut reader, &mut sink)?;
        } else {
            let db_dump_file =
                File::create_new(db_dump_file).map_err(MariaDbError::DestinationExists)?;
            let mut encoder = GzEncoder::new(db_dump_file, Compression::default());

            std::io::copy(&mut reader, &mut encoder)?;
            encoder.finish()?;
        }

        let exit_status = dump_process.wait().expect("mariadb-dump should be running");
        if !exit_status.success() {
            return Err(MariaDbError::DumpFailed(exit_status));
        }

        log::info!(target: "backend::mariadb-dump", "Finished Nextcloud database dump.");

        // TODO: cleanup of old backups

        Ok(())
    }
}
