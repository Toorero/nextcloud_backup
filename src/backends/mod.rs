pub mod config;
pub mod mariadb;
pub mod snapper;

pub use snapper::Snapper;

use crate::nextcloud::Nextcloud;

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
    fn backup(&mut self, nextcloud: &Nextcloud, dry_run: bool) -> Result<(), Self::Error>;
}
