use std::thread;

use nc_backup_lib::backends::{Backup, Config, MariaDb, Snapper};
use nc_backup_lib::cli::{Action, Cli};

use clap::Parser;
use nc_backup_lib::nextcloud::Nextcloud;
use nc_backup_lib::nextcloud::DEFAULT_INSTALLATION_ROOT;

fn main() {
    let cli = Cli::parse();
    assert!(
        matches!(cli.action.unwrap_or_default(), Action::Backup),
        "only support \"backup\" as action currently"
    );
    let backup_root = cli.backup_root;
    let dry_run = cli.dry_run;
    if dry_run {
        log::warn!("Running in dry-run mode");
    }

    // init logger
    let mut env_logger = env_logger::builder();
    if let Some(level) = cli.verbose {
        env_logger.filter_level(level);
    }
    env_logger.try_init().expect("env_logger should not fail");

    let nextcloud = Nextcloud::new(DEFAULT_INSTALLATION_ROOT.into())
        .expect("Nextcloud should be installed in /var/www/nextcloud");

    // FIXME: handle incomplete backups due to terminating signal

    // perform backup in parallel
    let snapper = if cli.snapper {
        let nextcloud = nextcloud.clone();
        let mut backend_snapper: Snapper = Snapper {
            cleanup_algorithm: cli.snapper_args.cleanup.into(),
            sync_destination: cli.snapper_args.sync_destination,
            incrementally: !cli.snapper_args.no_incrementally,
        };
        let snapper = thread::spawn(move || backend_snapper.backup(&nextcloud, dry_run));
        Some(snapper)
    } else {
        None
    };

    // snapper does not need maintenance mode
    if let Some(snapper) = snapper {
        let snapper_res = snapper.join().expect("no panic in backend snapper");
        if let Err(e) = snapper_res {
            log::error!(target: "backend::snapper", "Backup of Nextcloud data using Snapper resulted in a fatal error: {e}");
        }
    }

    nextcloud
        .occ()
        .enable_maintenance()
        .expect("maintenance should be enableable");
    let config = {
        let nextcloud = nextcloud.clone();
        let mut backend_config = Config::new(&backup_root);
        thread::spawn(move || backend_config.backup(&nextcloud, dry_run))
    };
    let mariadb = {
        let nextcloud = nextcloud.clone();
        let mut backend_mariadb = MariaDb::new(&backup_root);
        thread::spawn(move || backend_mariadb.backup(&nextcloud, dry_run))
    };
    let config_res = config.join().expect("no panic in backend config");
    if let Err(e) = config_res {
        log::error!(target: "backend::config", "Backup of Nextcloud config resulted in a fatal error: {e}");
    }
    let mariadb_res = mariadb.join().expect("no panic in backend mariadb");
    if let Err(e) = mariadb_res {
        log::error!(target: "backend::mariadb", "Backup of Nextcloud database resulted in a fatal error: {e}");
    }
    nextcloud
        .occ()
        .disable_maintenance()
        .expect("maintenance should be disableable");
}
