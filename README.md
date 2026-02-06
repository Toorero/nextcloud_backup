# Introduction

This tool creates a coherent backup of the Nextcloud config, database
and user data. The user data is deduplicated by utilizing [Snapper]
and btrfs snapshots.
The tool can be easily integrated into other tools duplicating the
created backup at different locations.

The backup itself can be run as the user running the Nextcloud instance.
A sample sandboxed systemd service and timer file are provided.

# Prerequisites

The program assumes the use of [MariaDB] as database backend of Nextcloud.
Create a `/home/nextcloud/.my.cnf` file for the `nextcloud` user to access
the database without the need to provide a password.

```sh
umask 077
cat << EOF > /home/nextcloud/.my.cnf
[client]
user=$(occ config:system:get dbuser)
password=$(occ config:system:get dbpassword)
EOF
sudo chown nextcloud:nextcloud /home/nextcloud/.my.cnf
```


The data directory of Nextcloud has to have a [Snapper] config associated with
and the `nextcloud` user setup as a user allowed to back up:
```sh
snapper create-config -t nextcloud_data $(occ config:system:get datadirectory)
sed -i \
    -e 's/ALLOW_USERS=.*/ALLOW_USERS="nextcloud"/' \
    -e 's/ALLOW_GROUPS=.*/ALLOW_GROUPS="nextcloud"/' \
    -e 's/SYNC_ACL=.*/SYNC_ACL="yes"/' \
    /etc/snapper/configs/nextcloud
```


# Installation

```sh
cargo install --path .
install ~/.local/share/cargo/bin/nc_backup /usr/local/bin
install nc_backup@.service nc_backup@.timer /etc/systemd/system
systemctl daemon-reload
```

# Usage

After installation, you can enable `nc_backup@.service`:
```sh
systemctl enable --now nc_backup@$(systemd-escape --path /nextcloud/backup).timer
```
The service parameter is the destination of the backup (backup root).
Ensure that the data directory of Nextcloud is writable by the 
hardened service. Include it as `ReadWritePath` if not already present:
```sh
echo ReadWritePaths=$(occ config:system:get datadirectory) >> nc_backup@.service
```

Alternatively you can run the backup program manually:
```sh
nc_backup --help
```

## 3-2-1

To achieve a 3-2-1 backup you should locate the backup destination on a different media.
Syncing of the created Snapper backups to media can be achieved using `snbk`.
A config at `/etc/snapper/backup-configs/nextcloud.json` could look like this:
```json
{
    "config": "nextcloud",
    "target-mode": "local",
    "automatic": true,
    "source-path": "/nextcloud/data",
    "target-path": "/nextcloud/backup/snapshots"
}
```
Make sure to enable `snapper-backup.timer`.

---

For a remote backup you can use `snbk` with a similar config or if you don't trust btrfs snapshots
all the way a deduplicatinng backup archiver like [Borg].

See the example Borg backup script on how you could achieve this. 
You can install backup script and its systemd-service like this:
```sh
install -m 755 nc_borg /usr/local/bin/nc_borg
install nc_borg.service /etc/systemd/system
systemctl daemon-reload
(echo "[Service]"; systemd-ask-password -n | systemd-creds encrypt --name "borg_passphrase" -p - -) \
    | systemctl edit --drop-in=borg_passphrase --stdin nc_borg.service
echo -e "[Service]\nEnvironment=BORG_REPO='ssh://borg@example.com'" \
    | systemctl edit --drop-in=env --stdin nc_borg.service
echo -e "[Unit]\nOnSuccess=nc_borg.service" \
    | systemctl edit --drop-in=nc_borg_hook --stdin nc_backup@.service
```

If you're using ssh keys just make
sure to prevent the corruption of the backup using the ssh key by enforcing
[append-only-mode](https://borgbackup.readthedocs.io/en/stable/usage/notes.html#append-only-mode).

[MariaDB]: https://mariadb.com/
[Snapper]: http://snapper.io/
[Borg]: https://www.borgbackup.org/
