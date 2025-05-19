mod occ;
use std::path::Path;
use std::path::PathBuf;

pub use occ::Occ;

const DEFAULT_DOCUMENT_ROOT: &str = "/var/www/nextcloud/";

#[derive(Debug, Clone)]
pub struct Nextcloud {
    pub occ: Occ,
    document_root: PathBuf,
}

impl Nextcloud {
    pub fn new(document_root: PathBuf) -> Nextcloud {
        assert!(
            document_root.is_dir(),
            "Nextcloud document root directory exists",
        );

        let occ_path = document_root.join("occ");
        let occ = Occ::with_occ_path(occ_path);

        Self { occ, document_root }
    }

    pub fn document_root(&self) -> &Path {
        self.document_root.as_path()
    }

    pub fn config(&self) -> PathBuf {
        self.document_root().join("config/config.php")
    }
}

impl Default for Nextcloud {
    fn default() -> Self {
        Self::new(DEFAULT_DOCUMENT_ROOT.into())
    }
}
