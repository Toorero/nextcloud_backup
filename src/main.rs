use std::thread;

use nc_backup_lib::backends::config::ConfigBackend;
use nc_backup_lib::backends::mariadb::MariaDBBackend;
use nc_backup_lib::backends::Backup;
use nc_backup_lib::backends::Snapper;
use nc_backup_lib::cli::Cli;

use clap::Parser;
use nc_backup_lib::nextcloud::Nextcloud;

fn main() {
    let cli = Cli::parse();
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

    let nextcloud = Nextcloud::default();

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

    nextcloud.occ.enable_maintenance();
    let config = {
        let nextcloud = nextcloud.clone();
        let mut backend_config = ConfigBackend::new(&backup_root);
        thread::spawn(move || backend_config.backup(&nextcloud, dry_run))
    };
    let mariadb = {
        let nextcloud = nextcloud.clone();
        let mut backend_mariadb = MariaDBBackend::new(&backup_root);
        thread::spawn(move || backend_mariadb.backup(&nextcloud, dry_run))
    };
    let config_res = config.join().expect("no panic in backend config");
    if let Err(e) = config_res {
        log::error!(target: "backend::config", "Backup of Netflix config resulted in a fatal error: {}", e);
    }
    let mariadb_res = mariadb.join().expect("no panic in backend mariadb");
    if let Err(e) = mariadb_res {
        log::error!(target: "backend::mariadb", "Backup of Netflix database resulted in a fatal error: {}", e);
    }
    nextcloud.occ.disable_maintenance();

    // snapper does not need maintenance mode
    if let Some(snapper) = snapper {
        let snapper_res = snapper.join().expect("no panic in backend snapper");
        if let Err(e) = snapper_res {
            log::error!(target: "backend::snapper", "Backup of Nextcloud data using Snapper resulted in a fatal error: {}", e);
        }
    }
}
