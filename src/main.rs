use std::collections::HashSet;
use std::thread;

use nc_backup_lib::backends::{BackendsConfig, Backup, Config, MariaDb};
use nc_backup_lib::cli::{Action, Backends, BackupArgs, Cli};

use clap::Parser;
use nc_backup_lib::nextcloud::Nextcloud;

fn main() {
    let cli = Cli::parse();
    let enabled_backends: HashSet<_> = cli.enabled_backends.into_iter().collect();
    let Action::Backup(BackupArgs {
        backup_root,
        update,
    }) = cli.action;

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
                return;
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
                return;
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
        thread::spawn(move || backend_snapper.backup(&nextcloud, dry_run))
    });

    let config = enabled_backends.get(&Backends::Config).map(|_| {
        let nextcloud = nextcloud.clone();
        let backend_config = Config::with_config(&backup_root, backends_config.config);
        thread::spawn(move || backend_config.backup(&nextcloud, dry_run))
    });

    let mariadb = enabled_backends.get(&Backends::MariaDb).map(|_| {
        let nextcloud = nextcloud.clone();
        let backend_mariadb = MariaDb::with_config(&backup_root, backends_config.mariadb);
        thread::spawn(move || backend_mariadb.backup(&nextcloud, dry_run))
    });

    // wait for completion of modules

    if let Some(snapper) = snapper {
        let snapper_res = snapper.join().expect("no panic in backend snapper");
        if let Err(e) = snapper_res {
            log::error!(target: "backend::snapper", "Backup of Nextcloud data using Snapper resulted in a fatal error: {e}");
        }
    }

    if let Some(config) = config {
        let config_res = config.join().expect("no panic in backend config");
        if let Err(e) = config_res {
            log::error!(target: "backend::config", "Backup of Nextcloud config resulted in a fatal error: {e}");
        }
    }

    if let Some(mariadb) = mariadb {
        let mariadb_res = mariadb.join().expect("no panic in backend mariadb");
        if let Err(e) = mariadb_res {
            log::error!(target: "backend::mariadb", "Backup of Nextcloud database resulted in a fatal error: {e}");
        }
    }

    if update {
        if let Err(e) = nextcloud.occ().update_apps(dry_run) {
            log::error!(target: "apps", "Updating the Nextcloud apps failed: {e}")
        }
    }

    nextcloud
        .occ()
        .disable_maintenance()
        .expect("maintenance should be disableable");
}
