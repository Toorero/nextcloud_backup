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

use crate::backends::config::ConfigConfig;
use crate::backends::mariadb::MariaDbConfig;
use crate::nextcloud::Nextcloud;

#[allow(missing_docs)]
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
}

#[derive(Debug, Default, serde::Serialize, serde::Deserialize)]
/// Configuration of all available backends.
pub struct BackendsConfig {
    /// Configuration of the [Config] backend.
    pub config: ConfigConfig,
    /// Configuration of the [MariaDb] backend.
    pub mariadb: MariaDbConfig,
    /// Configuration of the [Snapper] backend.
    pub snapper: Snapper,
}
