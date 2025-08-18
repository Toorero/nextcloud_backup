//! Implements backup of Nextcloud's `config.php` using [Config].

use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use chrono::Local;
use flate2::write::GzEncoder;
use flate2::Compression;
use regex::Regex;

use crate::backends::Backup;
use crate::nextcloud::Nextcloud;

const CONFIG_BACKUP_DEST: &str = "config/";

/// The [Config] backend allows you to backup Nextcloud's `config.php`.
pub struct Config {
    config_backup_dest: PathBuf,
}

impl Config {
    /// Create a new [Config] instance.
    pub fn new(backup_root: &Path) -> Self {
        let config_backup_root = backup_root.join(CONFIG_BACKUP_DEST);
        if config_backup_root.is_relative() {
            log::warn!(target: "backend::config", "config_backup_root is relative: {}", config_backup_root.display());
        }

        Self {
            config_backup_dest: config_backup_root,
        }
    }

    fn generate_config_backup_filename(&self) -> PathBuf {
        let timestamp = Local::now().format("%Y-%m-%dT%H-%M-%S");

        let path = self
            .config_backup_dest
            .join(format!("config-{timestamp}.php.gz"));
        assert!(!path.exists(), "config backup file should not exist prior");

        path
    }
}

impl Backup for Config {
    type Error = io::Error;

    fn backup(&mut self, nextcloud: &Nextcloud, dry_run: bool) -> Result<(), Self::Error> {
        let config_path = nextcloud.config();
        log::info!(target: "backend::config", "Create backup of Nextcloud config: {}", config_path.display());

        let config_file = File::open(config_path)?;
        let config_reader = BufReader::new(config_file);

        fs::create_dir_all(&self.config_backup_dest)?;
        let config_backup_file = self.generate_config_backup_filename();
        log::debug!(target: "backend::config", "Backup Nextcloud config to: {}", config_backup_file.display());
        let mut encoder = if dry_run {
            None
        } else {
            let config_backup_file = File::create_new(&config_backup_file)?;
            let encoder = GzEncoder::new(config_backup_file, Compression::default());
            Some(encoder)
        };

        // Mask dbpassword, since we don't need it when restoring.
        // https://github.com/nextcloud-snap/nextcloud-snap/blob/43ef350cff3d63a40e7868c408e792b5b0023375/src/import-export/bin/export-data#L64-L66
        let re = Regex::new(r"(dbpassword.*=>\s*).*,").unwrap();
        let mut replaced = false;
        for line in config_reader.lines() {
            let line = line?;

            let processed_line = if !replaced && re.is_match(&line) {
                replaced = true;
                log::trace!(target: "backend::config", "Masked dbpassword");
                re.replace(&line, "$1'DBPASSWORD',").into()
            } else {
                line
            };

            if let Some(ref mut encoder) = encoder {
                writeln!(encoder, "{processed_line}")?;
            }
        }

        if let Some(encoder) = encoder {
            encoder.finish()?;
        }

        if !replaced {
            log::warn!(target: "backend::config", "No dbpassword config entry found and masked!");
            //std::fs::remove_file(config_backup_file)?;
        }
        log::info!(target: "backend::config", "Finished backup of Nextcloud config");

        // TODO: cleanup of old backups

        Ok(())
    }
}
