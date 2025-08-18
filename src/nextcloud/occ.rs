use std::io;
use std::path::PathBuf;
use std::process::Command;

use derive_more::{Display, Error, From};

/// Error on determining the validity of the [Occ] path.
#[derive(Debug, Display, Error, From)]
pub enum OccPathError {
    /// Path to occ couldn't be found.
    #[display("Path to occ couldn't be located: {_0:#?}")]
    PathNotFound(#[error(ignore)] PathBuf),

    /// Generic [io::Error] on checking if the path exists occured.
    #[from]
    IoError(io::Error),
}

#[derive(Debug, Display, Error, From)]
/// Error on running an [Occ] command.
pub enum OccError {
    /// [Occ] command failed.
    #[display("Occ command {command:?} failed with error: {error}")]
    OccCommandFailed {
        /// [Command] that failed.
        #[error(ignore)]
        command: Box<Command>,
        #[error(ignore)]
        /// Captured stderr.
        error: String,
    },

    /// Generic [io::Error] on command execution.
    #[from]
    IoError(io::Error),
}

type Result<T> = std::result::Result<T, OccError>;

/// Access to the command-line interface of Nextcloud.
#[derive(Debug, Clone)]
pub struct Occ {
    /// Path to the occ php file.
    occ: PathBuf,
}

impl Occ {
    /// Create a new [Occ] instance.
    ///
    /// You should obtain an [Occ] instance through [Nextcloud::occ][super::Nextcloud::occ].
    pub fn new(occ_path: PathBuf) -> std::result::Result<Self, OccPathError> {
        if !occ_path.try_exists()? {
            return Err(OccPathError::PathNotFound(occ_path));
        }

        Ok(Self { occ: occ_path })
    }
}

impl Occ {
    fn execute_command(&self, command: &str, args: &[&str]) -> Result<String> {
        log::trace!(
            target: "nextcloud::occ",
            "Running: php {} --no-warnings {} {}",
            self.occ.as_path().display(),
            command,
            args.join(" ")
        );
        let mut occ_command = Command::new("php");
        occ_command
            .arg(self.occ.as_path())
            .arg("--no-warnings") // suppress maintenance mode is enabled warning
            .arg(command)
            .args(args);
        let occ_output = occ_command.output()?;

        let stdout = String::from_utf8_lossy(&occ_output.stdout);
        let stderr = String::from_utf8_lossy(&occ_output.stderr);

        if !occ_output.status.success() {
            return Err(OccError::OccCommandFailed {
                command: Box::new(occ_command),
                error: stderr.into(),
            });
        }

        // relay stderr
        if !stderr.is_empty() {
            log::warn!(target: "nextcloud::occ", "{stderr}");
        }

        Ok(stdout.trim_end().into())
    }

    /// Returns whether maintenance mode is enabled.
    pub fn maintenance(&self) -> Result<bool> {
        let msg = self.execute_command("maintenance:mode", &[])?;
        Ok(msg.contains("enabled"))
    }

    /// Enable the maintenance mode.
    pub fn enable_maintenance(&self) -> Result<()> {
        let _ = self.execute_command("maintenance:mode", &["--on"])?;

        assert!(self.maintenance()?, "maintenance should be enabled");
        log::debug!(target: "occ", "Maintenance Mode enabled.");

        Ok(())
    }

    /// Disable the maintenance mode.
    pub fn disable_maintenance(&self) -> Result<()> {
        let _ = self.execute_command("maintenance:mode", &["--off"])?;

        assert!(!self.maintenance()?, "maintenance should be disabled");
        log::debug!(target: "occ", "Maintenance Mode disabled.");

        Ok(())
    }

    /// Returns a path to the data directory of Nextcloud.
    pub fn data_directory(&self) -> Result<PathBuf> {
        let data_directory: PathBuf = self
            .execute_command("config:system:get", &["datadirectory"])?
            .into();
        assert!(
            data_directory.is_dir(),
            "nextcloud data directory should be an accesible directory"
        );

        Ok(data_directory)
    }

    /// Returns the name of the database.
    pub fn db_name(&self) -> Result<String> {
        self.execute_command("config:system:get", &["dbname"])
    }

    /// Returns the database user.
    pub fn db_user(&self) -> Result<String> {
        self.execute_command("config:system:get", &["dbuser"])
    }

    /// Updates all apps.
    pub fn update_apps(&self, show_only: bool) -> Result<()> {
        let opts = if show_only {
            // TODO: actually "show" something
            ["--show-only"]
        } else {
            ["--all"]
        };

        let update_log = self.execute_command("app:update", &opts)?;
        for line in update_log.lines() {
            log::info!(target: "nextcloud::occ", "Update Apps: {line}");
        }

        Ok(())
    }

    /// Send a notification to the Nextcloud `user`.
    pub fn notify(&self, user: &str, message: &str) -> Result<()> {
        let _ = self.execute_command("notification::generate", &[user, message])?;

        Ok(())
    }
}
