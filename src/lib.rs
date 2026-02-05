//! Library to backup your [Nextcloud][nc] installation.
//!
//! The library tries to follow the [official backup guidelines][nc_backup].
//! The different backup modules are located in the [`backends`] module.
//!
//! [nc]: https://nextcloud.com/
//! [nc_backup]: https://docs.nextcloud.com/server/latest/admin_manual/maintenance/backup.html

#![forbid(unsafe_code)]

pub mod backends;
pub mod cli;
pub mod nextcloud;
