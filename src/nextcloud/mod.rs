//! Access and manage Nextcloud installations.
//!
//! [Nextcloud] is the access point for managing your Nextcloud installation.
//! Additionally [Occ] exposes some of the commands of Nextcloud's command-line interface.

mod occ;

use derive_more::{Display, Error, From};
use std::path::{Path, PathBuf};

pub use occ::{Occ, OccError, OccPathError};

/// Default location of the `nextcloud/` folder of a Nextcloud installation on Ubuntu Linux.
pub const DEFAULT_INSTALLATION_ROOT: &str = "/var/www/nextcloud/";

/// A Nextcloud instance.
#[derive(Debug, Clone)]
pub struct Nextcloud {
    occ: Occ,
    document_root: PathBuf,
}

#[derive(Display, Debug, Error, From)]
/// Errors possible on creating a [Nextcloud] instance.
pub enum NextcloudError {
    /// The installation folder of Nextcloud couldn't be located.
    #[display("Nextcloud installation directory couldn't be located: {_0:#?}")]
    InstalltionNotFound(#[error(ignore)] PathBuf),
    /// Nextcloud's command-line interface couldn't be located.
    #[from]
    Occ(OccPathError),
}

impl Nextcloud {
    /// Create a new [Nextcloud] instance.
    ///
    /// You can use the [DEFAULT_INSTALLATION_ROOT] if your Nextcloud installation is
    /// deployed in `/var/www/nextcloud` as on Ubuntu Linux.
    ///
    /// # Example
    ///
    /// ```
    /// let nc = Nextcloud::new(DEFAULT_INSTALLATION_ROOT.into());
    /// assert!(nc.is_ok());
    /// ```
    pub fn new(installation_root: PathBuf) -> Result<Nextcloud, NextcloudError> {
        // TODO: Handle io::Error
        if !installation_root.is_dir() {
            return Err(NextcloudError::InstalltionNotFound(installation_root));
        }

        let occ_path = installation_root.join("occ");
        let occ = Occ::new(occ_path)?;

        Ok(Self {
            occ,
            document_root: installation_root,
        })
    }

    /// Get the root document folder of the Nextcloud installation.
    ///
    /// The root document folder is where the files of the currently installed
    /// version of Nextcloud are located.
    ///
    /// # Example
    ///
    /// ```
    /// let nc = Nextcloud::new(DEFAULT_INSTALLATION_ROOT.into()).unwrap();
    /// assert_eq!(nc.document_root().to_str(), Some("/var/www/nextcloud"));
    /// ```
    pub fn document_root(&self) -> &Path {
        self.document_root.as_path()
    }

    /// Get the path to the `config.php` of Nextcloud.
    ///
    /// # Example
    ///
    /// ```
    /// let nc = Nextcloud::new(DEFAULT_INSTALLATION_ROOT.into()).unwrap();
    /// assert_eq!(nc.config().to_str(), Some("/var/www/nextcloud/config/config.php"));
    /// ```
    pub fn config(&self) -> PathBuf {
        self.document_root().join("config/config.php")
    }

    /// The command-line interface of the Nextcloud instance.
    pub fn occ(&self) -> &Occ {
        &self.occ
    }
}
