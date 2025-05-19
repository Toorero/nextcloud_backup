use std::path::PathBuf;
use std::process::Command;

const DEFAULT_OCC_PATH: &str = "/var/www/nextcloud/occ";

/// Interaction with the Nextcloud instance using the [`occ` command].
#[derive(Debug, Clone)]
pub struct Occ {
    /// Path to the occ php file.
    occ: PathBuf,
}

impl Default for Occ {
    fn default() -> Self {
        Self::with_occ_path(PathBuf::from(DEFAULT_OCC_PATH))
    }
}

impl Occ {
    pub fn with_occ_path(occ_path: PathBuf) -> Self {
        assert!(occ_path.exists(), "occ php file should exist");

        Self { occ: occ_path }
    }
}

impl Occ {
    fn execute_command(&self, command: &str, args: &[&str]) -> String {
        let occ_output = Command::new("php")
            .arg(self.occ.as_path())
            .arg("--no-warnings") //suppress maintenance mode is enabled warning
            .arg(command)
            .args(args)
            .output()
            .expect("Failed to execute occ command");
        assert!(occ_output.status.success(), "occ command failed");

        let stdout = String::from_utf8_lossy(&occ_output.stdout);
        let stderr = String::from_utf8_lossy(&occ_output.stderr);

        // relay stderr
        if !stderr.is_empty() {
            log::warn!(target: "nextcloud::occ", "{}", stderr );
        }

        stdout.trim_end().into()
    }

    pub fn maintenance(&self) -> bool {
        let msg = self.execute_command("maintenance:mode", &[]);
        msg.contains("enabled")
    }
    pub fn enable_maintenance(&self) {
        let _ = self.execute_command("maintenance:mode", &["--on"]);

        assert!(self.maintenance(), "maintenance should be enabled");
        log::debug!(target: "occ", "Maintenance Mode enabled.");
    }

    pub fn disable_maintenance(&self) {
        let _ = self.execute_command("maintenance:mode", &["--off"]);

        assert!(!self.maintenance(), "maintenance should be disabled");
        log::debug!(target: "occ", "Maintenance Mode disabled.");
    }

    pub fn data_directory(&self) -> PathBuf {
        let data_directory: PathBuf = self
            .execute_command("config:system:get", &["datadirectory"])
            .into();
        assert!(
            data_directory.is_dir(),
            "nextcloud data directory should be an accesible directory"
        );

        data_directory
    }

    pub fn db_name(&self) -> String {
        self.execute_command("config:system:get", &["dbname"])
    }
    pub fn db_user(&self) -> String {
        self.execute_command("config:system:get", &["dbuser"])
    }

    pub fn update_apps(&self, show_only: bool) {
        let opts = if show_only {
            ["--show-only"]
        } else {
            ["--all"]
        };

        let update_log = self.execute_command("app:update", &opts);
        for line in update_log.lines() {
            log::info!(target: "nextcloud::occ", "Update Apps: {line}");
        }
    }

    pub fn notify(&self, user: &str, message: &str) {
        let _ = self.execute_command("notification::generate", &[user, message]);
    }
}
