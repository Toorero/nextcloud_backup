//! Backend modules for performing individual backup tasks.
//!
//! Currently the following backends are implemented:
//!
//! - [MariaDb]: Compressed backup of the Nextcloud MariaDB tables.
//! - [Snapper]: Atomic backup of user-data of the Nextcloud.
//! - [Config]: Backup of Nextcloud's `config.php`

pub mod config;
pub mod mariadb;
pub mod snapper;

pub use config::Config;
pub use mariadb::MariaDb;
pub use snapper::Snapper;

use crate::nextcloud::Nextcloud;
use crate::util::retention::RetentionConfig;

#[allow(missing_docs)]
/// Generic backup backend.
pub trait Backup {
    /// Error that may happen on backup.
    type Error;

    /// Backups data managed by the implementation.
    ///
    /// # Dry Run
    ///
    /// On a dry run (`dry_run=true`) no files are altered.
    /// This does include folders and other special files.
    ///
    /// Instead sanity checks are performed to determine if a "real" backup
    /// would succeed under the present conditions.
    fn backup(&self, nextcloud: &Nextcloud, dry_run: bool) -> Result<(), Self::Error>;

    /// Applies the [RetentionConfig] to all backups created by the [Backup].
    fn retention(
        &self,
        nextcloud: &Nextcloud,
        cfg: &RetentionConfig,
        dry_run: bool,
    ) -> Result<(), Self::Error>;
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
/// Configuration of all available backends.
pub struct BackendsConfig {
    /// Configuration of the [Snapper] backend.
    pub snapper: Snapper,

    /// Retention config.
    pub retention: RetentionConfig,
}
