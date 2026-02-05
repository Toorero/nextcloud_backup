use std::collections::HashSet;
use std::process::ExitCode;
use std::thread;

use nc_backup_lib::backends::{BackendsConfig, Backup, Config, MariaDb};
use nc_backup_lib::cli::{Action, Backends, BackupArgs, Cli};

use clap::Parser;
use nc_backup_lib::nextcloud::Nextcloud;

fn main() -> ExitCode {
    let cli = Cli::parse();
    let enabled_backends: HashSet<_> = cli.enabled_backends.into_iter().collect();

    // init logger
    let mut env_logger = env_logger::builder();
    if let Some(level) = cli.verbose {
        env_logger.filter_level(level);
    }
    env_logger.try_init().expect("env_logger should not fail");

    let backends_config: BackendsConfig = match std::fs::read(&cli.config) {
        Ok(config_str) => match toml::from_slice(&config_str) {
            Err(e) => {
                log::error!("Reading the config file failed: {e}");
                return ExitCode::from(255);
            }
            Ok(cfg) => cfg,
        },
        Err(e) => {
            if std::fs::exists(&cli.config).is_ok_and(|b| !b) {
                log::debug!(
                    "Writing default config to {} because it doesn't exist yet",
                    cli.config.display()
                );
                let default_config = BackendsConfig::default();
                let config_str = toml::to_string_pretty(&default_config)
                    .expect("default config should be serializable");
                if let Err(e) = std::fs::write(&cli.config, config_str) {
                    log::warn!(
                        "Writing default config to {} failed {e}",
                        cli.config.display(),
                    );
                }

                default_config
            } else {
                log::error!("Reading the config file failed: {e}");
                return ExitCode::from(255);
            }
        }
    };

    let dry_run = cli.dry_run;
    if dry_run {
        log::warn!("Running in dry-run mode");
    }

    let nextcloud = Nextcloud::new(cli.document_root)
        .expect("Nextcloud should be installed in {cli.document_root}");

    // FIXME: handle incomplete backups due to terminating signal

    nextcloud
        .occ()
        .enable_maintenance()
        .expect("maintenance should be enableable");

    // spawn threads for different components (Snapper, Config, MariaDB)

    let snapper = enabled_backends.get(&Backends::Snapper).map(|_| {
        let nextcloud = nextcloud.clone();
        let backend_snapper = backends_config.snapper;

        match cli.action {
            Action::Backup(..) => {
                thread::spawn(move || backend_snapper.backup(&nextcloud, dry_run))
            }
            Action::Retain => thread::spawn(move || {
                backend_snapper.retention(&nextcloud, &backends_config.retention, dry_run)
            }),
        }
    });

    let config = enabled_backends.get(&Backends::Config).map(|_| {
        let nextcloud = nextcloud.clone();
        let backend_config = Config::new(&cli.backup_root);
        match cli.action {
            Action::Backup(..) => thread::spawn(move || backend_config.backup(&nextcloud, dry_run)),
            Action::Retain => thread::spawn(move || {
                backend_config.retention(&nextcloud, &backends_config.retention, dry_run)
            }),
        }
    });

    let mariadb = enabled_backends.get(&Backends::MariaDb).map(|_| {
        let nextcloud = nextcloud.clone();
        let backend_mariadb = MariaDb::new(&cli.backup_root);
        match cli.action {
            Action::Backup(..) => {
                thread::spawn(move || backend_mariadb.backup(&nextcloud, dry_run))
            }
            Action::Retain => thread::spawn(move || {
                backend_mariadb.retention(&nextcloud, &backends_config.retention, dry_run)
            }),
        }
    });

    // wait for completion of modules
    let mut exit_code = 0;

    if let Some(snapper) = snapper {
        let snapper_res = snapper.join().expect("no panic in backend snapper");
        if let Err(e) = snapper_res {
            log::error!(target: "backend::snapper", "Fatal error: {e}");
            exit_code += 1 << 1;
        }
    }

    if let Some(config) = config {
        let config_res = config.join().expect("no panic in backend config");
        if let Err(e) = config_res {
            log::error!(target: "backend::config", "Fatal error: {e}");
            exit_code += 1 << 2;
        }
    }

    if let Some(mariadb) = mariadb {
        let mariadb_res = mariadb.join().expect("no panic in backend mariadb");
        if let Err(e) = mariadb_res {
            log::error!(target: "backend::mariadb", "Fatal error: {e}");
            exit_code += 1 << 3;
        }
    }

    if let Action::Backup(BackupArgs { update: true, .. }) = cli.action {
        if let Err(e) = nextcloud.occ().update_apps(dry_run) {
            log::error!(target: "apps", "Updating the Nextcloud apps failed: {e}");
            exit_code += 1;
        }
    }

    nextcloud
        .occ()
        .disable_maintenance()
        .expect("maintenance should be disableable");

    if exit_code != 0 {
        return ExitCode::from(exit_code);
    }
    ExitCode::SUCCESS
}
