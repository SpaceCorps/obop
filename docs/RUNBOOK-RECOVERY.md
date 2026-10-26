# Recovery runbook: losing data, a host or a key

- **Status:** working document, 2026-10-25. Decision record: D73 in [DECISIONS.md](DECISIONS.md). The commands are in [DEPLOY.md](DEPLOY.md) ("Backups", "Upgrades"); this page is the order to use them in, who does what, and what to decide on the way.
- **What it covers:** one instance database (the "main" plane: one store of your data that you back up, drill and attest on its own; see the glossary) and the keys around it. Other stores that may hold your data (an object store for uploaded files beyond `OBOP_BLOB_DIR`, a reverse proxy, DNS) need their own lines in your copy of this page.
- **What it is not:** a promise. The recovery point and the recovery time below are **measured by your drills on your data and your hardware**, written into the table by the person who ran them. Nothing in the product guarantees a number, and none of the numbers in this repository was measured against a managed PostgreSQL service.

## 1. The numbers this runbook is judged by

Fill these in. An empty cell means "not measured yet", and that is a finding.

| What | Target (yours to set) | Last measured | Where it comes from |
|---|---|---|---|
| Recovery point, logical backup (how much can be lost) | | | `obop storage show`: "measured recovery point" of the newest passed drill; at worst the backup interval |
| Recovery point, with WAL archiving | | | `archive_timeout` (60 s or less) plus the archiver lag in `pg_stat_archiver`; proved by the point-in-time drill |
| Recovery time, logical restore | | | `obop drill` prints "recovery time"; `restore_drill` records keep it |
| Recovery time, loss of primary on a new host | | | the half-yearly drill, timed by a person from "primary lost" to "serving" |
| Restore of a base backup plus WAL | | | the quarterly point-in-time drill |

**Rule for the primary path.** The logical restore is **one transaction** and is not resumable. If its measured time at your data size is more than **half of your recovery-time target**, the primary recovery path is the physical one (base backup plus WAL, section 5) and the logical backup is for portability, migration and a second opinion only. Write down here which it is: `__________`. Above about 10 GB a resumable chunked restore is required and is not built (D73).

## 2. Roles

| Role | Does | Must hold |
|---|---|---|
| Incident lead | Decides the path (section 3), keeps the log, says when it is over | the authority to stop the service |
| Operator | Runs the commands | database owner access, the backup volume, the off-host copy's read credential |
| Key custodian | Provides keys, and says honestly what is lost if one is gone | `OBOP_BACKUP_KEY` and `OBOP_SECRET_KEY` from the secret store; a second custodian for each |
| Data owner | Decides which loss is acceptable (a day, an hour) and approves a rollback | the business's authority over the data |
| Communicator | Tells users and, where the law asks, authorities | the contact lists |

Write the names and numbers into your copy. A runbook with a role nobody holds is a plan to improvise.

## 3. Decide

1. **Stop writes if data may still be getting worse** (a bad import is running, a migration half-applied): stop the processes that write (`obop serve`, scheduled jobs). Reads can wait.
2. **What happened?**

| Symptom | Path |
|---|---|
| A migration or module update broke data, and it was backed up first (the ledger names the backup: `obop migrate status`) | **A. Roll back by restore** (section 4) |
| A wrong bulk change or deletion, and you know when it happened | **C. Point-in-time recovery** to just before it (section 5), if WAL archiving was on; else the newest backup before it, **A** |
| The primary host or its volume is gone | **B. New host from the off-host copy** (section 6), or **C** when WAL is also off-host |
| Corruption on disk (checksum errors) | stop; **C** or **B**; do not repair in place |
| A key is lost | **D** (section 7), first, before restoring anything |
| Suspected compromise | do not restore over the evidence: copy the volume, then rebuild on a new host (**B**), rotate every key (DEPLOY.md, "If a key leaks") and the database password |

3. **Choose the recovery point** with the data owner: the newest backup is not always the right one. `obop backup list` shows what exists; a restore point `pre-migration-<version>` is listed in the WAL archive of a migration that ran with archiving on.

## 4. A. Roll back a migration by restore

Forward-only migrations: the way back is the verified backup that came before (D59, D73).

1. Stop every process that uses the database. Note the failed run: `obop migrate status --db URL` (the ledger rows, with the backup file name and any restore point).
2. `obop backup verify FILE` (the key must be configured; exit 3 and `restore_key_required` mean it is not; exit 4 and `backup_not_sealed` mean the file is plain although a key is configured: do not trust it unless you know where it has been, then `--allow-plain`. **Only restore a file that says AUTHENTICATED**, or a `.obbk` you took yourself: a plain pair proves nothing about who wrote it.)
3. `obop migrate rollback --db URL --from FILE --reason "what went wrong, in one line" --yes`. The migration lock is taken and the database is replaced by the backup; the reason, the backup and what the discarded ledger said are recorded in the ledger and the security log in the same transaction as the restore (a backup of another database is refused unless `--other-database`; if an uploaded file cannot be written afterwards the command says `restore_files_failed`, exit 5: the records are in, run it again with `--force`).
4. Start the obop **version that matches the backup's schema** (the old binary or image). `obop migrate status` agrees with it; `obop storage check`.
5. Decide with the release owner whether the new version may be tried again. The next `obop migrate run` is gated again (backup, plan, rehearsal): rehearse the fix first.

## 5. C. Point-in-time recovery (physical path)

Needs WAL archiving to be on and healthy before the loss (`obop storage check`; DEPLOY.md, "PostgreSQL itself"). Sketch with pgBackRest; use your tool's equivalent:

1. Stop PostgreSQL on the target; move the data directory aside (do not delete it yet).
2. `pgbackrest --stanza=obop --type=time "--target=2026-10-25 11:59:00+00" --target-action=promote restore`, or to a migration's named point: `--type=name --target=pre-migration-3`.
3. Start PostgreSQL; watch the log until the target is reached and the server is promoted.
4. `obop migrate status --db URL`, `obop storage check`, and the drill's checks on the result: take a backup (`obop backup run`) and drill it (`obop drill FILE --scratch temp`).
5. Records written after the target are gone. Tell the data owner which, from the application logs and the security log of the old data directory.

## 6. B. A new host from the off-host copy alone

This is the half-yearly drill too (section 8). Everything comes from outside the lost host.

1. A new PostgreSQL host with TLS and the settings of DEPLOY.md; an empty database and role. `obop` of a version **not older** than the backup's.
2. Keys from the secret store: `OBOP_BACKUP_KEY` (or the key that sealed the backup) and `OBOP_SECRET_KEY` (and the previous keys, if the backup is older than a rotation).
3. Fetch the newest backup **and its manifest** from the copy (the data file alone is not enough).
4. `obop backup verify FILE`; `obop restore --db NEW_URL --from FILE --dry-run`; then without `--dry-run`.
5. `obop migrate status`; if the backup is from an older schema `obop migrate run` (gated, DEPLOY.md), or drill it first with `obop drill FILE --scratch URL --migrate`.
6. Set `OBOP_PUBLIC_URL`, the TLS and proxy settings. **Passkeys are bound to the address**: if the address changed people re-enrol them (passwords and authenticator apps are unaffected).
7. Start `obop serve`; `GET /readyz` answers 200; sign in; spot-check a record that was changed on the last day.
8. Turn the scheduled backup back on (`OBOP_BACKUP_SCHEDULE`), attest the new host's storage (`obop storage attest ...`), and run `obop drill` on the first new backup. Time the whole thing and put it in section 1.

## 7. D. A key is lost

| Lost | Consequence | Do |
|---|---|---|
| The backup key, and another copy exists | none | restore the key from the second custodian; note the incident |
| The backup key, no copy | **every backup sealed with it is unreadable; there is no backdoor** | the live database is unaffected: take a new backup with a new key at once (`obop backup run`), then protect the keys properly. Only the live database can be recovered from now on |
| `OBOP_SECRET_KEY` | the data restores; two-factor seeds and connection credentials cannot be opened | DEPLOY.md, "If the key is lost": set a new key, `obop secrets rotate --skip-unopenable` to see what is lost, `obop admin reset-mfa`, enter credentials again |
| Both | sealed backups unreadable and secrets lost | as the first two rows together |
| The database password | the server cannot connect | reset it at PostgreSQL, update the secret store and the URL |

## 8. Drills: what, how often, how to record

A copy you have not restored is not a backup. Each drill is recorded (`obop drill ... --record-in URL`, which `obop storage check` reads: it fails when the newest passed drill is older than 35 days).

| Drill | How often | How | Passes when |
|---|---|---|---|
| Logical restore from the off-host copy | monthly | fetch the newest backup and manifest from the copy to a scratch host; `deploy/restore-drill.sh FILE` (a scratch PostgreSQL, a server on the copy) with `DRILL_RECORD_IN` (recorded after the server step: a server that did not answer is recorded as FAILED) | every check passes; the time is written to section 1 |
| Point-in-time | quarterly | on a fresh server: write a marker row at T0, write more rows, restore to T0 + 1 s, expect the marker and none of the later rows; also a restore point 34 days back | the marker is there, the later rows are not, `obop storage check` and the drill's checks pass |
| Loss of primary | twice a year | section 6 from the off-host copy alone, on a new host, with a stopwatch | serving, with the measured recovery time and the age of the newest backup (recovery point) in section 1 |
| Failure injection | each time the backup job or key handling changes (CI does it on every change) | the wrong key (`restore_key_required`, `obop drill` itself exits 3), a plain copy where a key is configured (exit 4), a corrupt copy, a truncated copy: `obop drill` must fail for each, loudly | all three fail the drill |

After each drill: the record exists (`obop storage show`), the table in section 1 is updated, and anything that surprised you becomes a line in this page.

## 9. After any recovery

- The ledger and the log tell the story: `obop migrate status`, the security log (`migration_rollback`, `migration_run`, `storage_attested`, `drill_recorded`, each under the operating system user that ran the command).
- Re-take a backup and drill it; check the schedule is on; check the alerts (DEPLOY.md, "Alerts") are quiet.
- Write down what was lost, for how long, and what would have prevented it.

## 10. Not delivered / not tried

- A resumable chunked restore (required above 10 GB). The records of a restore are one transaction; the uploaded files follow it.
- Erasures recorded after a backup are not replayed on restore (there is no erasure ledger yet): after a restore, re-apply erasures from your own records before going live.
- Recovery of other stores (object storage beyond the blob directory, other planes).
- Any managed PostgreSQL service, the Sliplane cell, a real object store with object lock, a real pgBackRest or wal-g: the settings and checks were written from their documentation and run against the stock `postgres:17` image and a fake server.
