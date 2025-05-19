use std::fs::{self, File};
use std::io::{self, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use chrono::Local;
use flate2::write::GzEncoder;
use flate2::Compression;

use crate::backends::Backup;
use crate::nextcloud::Nextcloud;

const DB_DUMP_DEST: &str = "db/";

pub struct MariaDBBackend {
    db_dump_dest: PathBuf,
}
impl MariaDBBackend {
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

impl Backup for MariaDBBackend {
    type Error = io::Error;

    fn backup(&mut self, nextcloud: &Nextcloud, dry_run: bool) -> Result<(), Self::Error> {
        let table_name = nextcloud.occ.db_name();
        let table_usr = nextcloud.occ.db_user();
        log::info!(target: "backend::mariadb", "Create database dump of the Nextcloud table: {}", table_name);
        log::trace!(target: "backend::mariadb", "Using dbuser '{}' for backup", table_usr);

        fs::create_dir_all(&self.db_dump_dest)?;
        let db_dump_file = self.generate_db_dump_filename();
        log::debug!(target: "backend::mariadb", "Save Nextcloud database dump at: {}", db_dump_file.display());

        let mut dump_process = Command::new("mariadb-dump")
            .arg("--opt") // sensible dump defaults
            .arg("--single-transaction")
            .arg(format!("--user={table_usr}"))
            .arg(table_name)
            .stdout(Stdio::piped())
            .spawn()?;
        log::trace!(target: "backend::mariadb", "Started mariadb-dump process.");

        // compress and capture stdout of mariadb-dump
        let stdout = dump_process.stdout.take().unwrap();
        let mut reader = BufReader::new(stdout);
        if dry_run {
            log::trace!(target: "backend::mariadb", "Discarding output of mariadb-dump on dry-run");
            let mut sink = io::sink();
            std::io::copy(&mut reader, &mut sink)?;
        } else {
            let db_dump_file = File::create_new(db_dump_file)?;
            let mut encoder = GzEncoder::new(db_dump_file, Compression::default());

            std::io::copy(&mut reader, &mut encoder)?;
            encoder.finish()?;
        }

        let exit_status = dump_process.wait()?;
        assert!(
            exit_status.success(),
            "mariadb-dump should execute successfully"
        );

        log::info!(target: "backend::mariadb-dump", "Finished Nextcloud database dump.");

        // TODO: cleanup of old backups

        Ok(())
    }
}
