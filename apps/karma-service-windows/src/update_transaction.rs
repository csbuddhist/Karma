//! Write-ahead transaction for an update. The Windows backend atomically
//! replaces each installed file and retains its predecessor. Recovery is
//! idempotent, including interruption during rollback itself.

use std::{
    collections::BTreeSet,
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};

use karma_update::{
    MANIFEST_FILE, ReleaseManifest, SIGNATURE_FILE, UpdateError, reject_links, safe_relative_path,
};
use serde::{Deserialize, Serialize};

pub const BACKUP: &str = ".karma-update-backup";
pub const PREPARED: &str = ".karma-update-new";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Phase {
    Applying,
    Applied,
    Committed,
}

pub fn requires_recovery(phase: Phase, worker_active: bool) -> bool {
    phase == Phase::Applying || phase == Phase::Applied && !worker_active
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub relative: String,
    pub had_original: bool,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Plan {
    pub install: PathBuf,
    pub installed_version: String,
    pub version: String,
    pub phase: Phase,
    pub files: Vec<Entry>,
    pub retired: Vec<String>,
}

pub type Replace = fn(&Path, &Path, Option<&Path>) -> io::Result<()>;

pub fn retired_files(previous: Vec<String>, manifest: &ReleaseManifest) -> Vec<String> {
    let current: BTreeSet<_> = manifest
        .files
        .keys()
        .map(|path| path.to_ascii_lowercase())
        .collect();
    previous
        .into_iter()
        .filter(|path| !current.contains(&path.to_ascii_lowercase()))
        .collect()
}

/// Before the write-ahead journal exists no installed file has been changed.
/// These reserved copies are therefore safe to discard after a failed prepare.
pub fn cleanup_unjournaled(install: &Path) -> Result<(), UpdateError> {
    for directory in [BACKUP, PREPARED] {
        let path = install.join(directory);
        if path.exists() {
            karma_update::remove_directory(&path)?;
        }
    }
    Ok(())
}

pub fn prepare(
    install: &Path,
    staged: &Path,
    manifest: &ReleaseManifest,
    installed_version: &str,
    retired: Vec<String>,
) -> Result<Plan, UpdateError> {
    reject_links(install)?;
    for directory in [BACKUP, PREPARED] {
        if install.join(directory).exists() {
            return Err(UpdateError::UnsafePath);
        }
        fs::create_dir(install.join(directory))?;
    }
    let mut names: Vec<_> = manifest.files.keys().cloned().collect();
    // Replace the executable that SCM starts last. At all points it is an
    // ordinary executable; replacement never moves it out of the way first.
    names.sort_by_key(|name| name == "KarmaService.exe");
    names.extend([MANIFEST_FILE.into(), SIGNATURE_FILE.into()]);
    let mut files = Vec::new();
    for relative in names {
        let path = safe_relative_path(&relative).ok_or(UpdateError::InvalidManifest)?;
        let target = install.join(&path);
        reject_links(&target)?;
        let original = match fs::symlink_metadata(&target) {
            Ok(metadata) => Some(metadata),
            Err(error) if error.kind() == io::ErrorKind::NotFound => None,
            Err(error) => return Err(error.into()),
        };
        if original
            .as_ref()
            .is_some_and(|metadata| !metadata.is_file())
        {
            return Err(UpdateError::UnsafePath);
        }
        if original.is_some() {
            let backup = install.join(BACKUP).join(&path);
            fs::create_dir_all(backup.parent().ok_or(UpdateError::UnsafePath)?)?;
            let mut input = File::open(&target)?;
            let mut output = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(backup)?;
            io::copy(&mut input, &mut output)?;
            output.sync_all()?;
        }
        let prepared = install.join(PREPARED).join(&path);
        fs::create_dir_all(prepared.parent().ok_or(UpdateError::UnsafePath)?)?;
        let mut input = File::open(staged.join(&path))?;
        let mut output = OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(prepared)?;
        io::copy(&mut input, &mut output)?;
        output.sync_all()?;
        files.push(Entry {
            relative,
            had_original: original.is_some(),
        });
    }
    Ok(Plan {
        install: install.into(),
        installed_version: installed_version.into(),
        version: manifest.version.clone(),
        phase: Phase::Applying,
        files,
        retired,
    })
}

pub fn write_plan(path: &Path, plan: &Plan, replace: Replace) -> Result<(), UpdateError> {
    reject_links(path)?;
    let pending = path.with_extension("pending");
    if pending.exists() {
        fs::remove_file(&pending)?;
    }
    let mut file = OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&pending)?;
    file.write_all(&serde_json::to_vec(plan).map_err(|_| UpdateError::InvalidManifest)?)?;
    file.sync_all()?;
    drop(file);
    replace(&pending, path, None)?;
    Ok(())
}

pub fn read_plan(path: &Path) -> Result<Plan, UpdateError> {
    reject_links(path)?;
    let metadata = fs::metadata(path)?;
    if metadata.len() > 256 * 1024 {
        return Err(UpdateError::TooLarge);
    }
    let plan: Plan =
        serde_json::from_slice(&fs::read(path)?).map_err(|_| UpdateError::InvalidManifest)?;
    if !plan.install.is_absolute()
        || plan.files.len() > 514
        || plan.retired.len() > 512
        || plan
            .files
            .iter()
            .any(|entry| safe_relative_path(&entry.relative).is_none())
        || plan
            .retired
            .iter()
            .any(|entry| safe_relative_path(entry).is_none())
    {
        return Err(UpdateError::InvalidManifest);
    }
    reject_links(&plan.install)?;
    Ok(plan)
}

pub fn apply(plan: &Plan, replace: Replace) -> Result<(), UpdateError> {
    for entry in &plan.files {
        let path = safe_relative_path(&entry.relative).ok_or(UpdateError::InvalidManifest)?;
        let target = plan.install.join(&path);
        let source = plan.install.join(PREPARED).join(&path);
        let backup = plan.install.join(BACKUP).join(&path);
        reject_links(&target)?;
        reject_links(&source)?;
        fs::create_dir_all(target.parent().ok_or(UpdateError::UnsafePath)?)?;
        fs::create_dir_all(backup.parent().ok_or(UpdateError::UnsafePath)?)?;
        replace(
            &source,
            &target,
            entry.had_original.then_some(backup.as_path()),
        )?;
    }
    Ok(())
}

pub fn rollback(plan: &Plan, replace: Replace) -> Result<(), UpdateError> {
    for entry in plan.files.iter().rev() {
        let path = safe_relative_path(&entry.relative).ok_or(UpdateError::InvalidManifest)?;
        let target = plan.install.join(&path);
        let backup = plan.install.join(BACKUP).join(&path);
        reject_links(&target)?;
        reject_links(&backup)?;
        if backup.is_file() && target.is_file() && same_contents(&backup, &target)? {
            // Never replaced (or already restored): an unchanged installed file
            // may still be mapped by a running executable, so do not overwrite it.
            // 未被替换的文件可能仍被运行中的程序占用，不能覆盖，只丢弃其备份。
            fs::remove_file(backup)?;
        } else if backup.is_file() {
            replace(&backup, &target, None)?;
        } else if !entry.had_original && target.exists() {
            // No predecessor existed: this name belongs only to this update.
            fs::remove_file(target)?;
        }
    }
    Ok(())
}

fn same_contents(left: &Path, right: &Path) -> io::Result<bool> {
    use std::io::Read as _;
    if fs::metadata(left)?.len() != fs::metadata(right)?.len() {
        return Ok(false);
    }
    let (mut left, mut right) = (File::open(left)?, File::open(right)?);
    let (mut a, mut b) = (vec![0_u8; 64 * 1024], vec![0_u8; 64 * 1024]);
    loop {
        let read = left.read(&mut a)?;
        if read == 0 {
            return Ok(true);
        }
        right.read_exact(&mut b[..read])?;
        if a[..read] != b[..read] {
            return Ok(false);
        }
    }
}

pub fn cleanup(plan: &Plan) -> Result<(), UpdateError> {
    if plan.phase == Phase::Committed {
        for relative in &plan.retired {
            let path = plan
                .install
                .join(safe_relative_path(relative).ok_or(UpdateError::InvalidManifest)?);
            reject_links(&path)?;
            if path.is_file() {
                fs::remove_file(path)?;
            }
        }
    }
    for directory in [BACKUP, PREPARED] {
        let path = plan.install.join(directory);
        if path.exists() {
            karma_update::remove_directory(&path)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn replace(source: &Path, target: &Path, backup: Option<&Path>) -> io::Result<()> {
        if let Some(backup) = backup {
            assert!(
                backup.is_file(),
                "the predecessor is durable before the journal"
            );
        }
        fs::rename(source, target)
    }

    fn fixture() -> (tempfile::TempDir, Plan) {
        let root = tempfile::tempdir().unwrap();
        let canonical = root.path().canonicalize().unwrap();
        let install = canonical.join("install");
        let staged = canonical.join("staged");
        fs::create_dir(&install).unwrap();
        fs::create_dir(&staged).unwrap();
        fs::write(install.join("KarmaService.exe"), b"old-service").unwrap();
        for (name, bytes) in [
            ("KarmaService.exe", "new-service"),
            ("added.dll", "new-dll"),
            (MANIFEST_FILE, "manifest"),
            (SIGNATURE_FILE, "sig"),
        ] {
            fs::write(staged.join(name), bytes).unwrap();
        }
        let manifest = ReleaseManifest {
            format: 1,
            product: "karma".into(),
            version: "0.2.4".into(),
            files: BTreeMap::from([
                ("KarmaService.exe".into(), "0".repeat(64)),
                ("added.dll".into(), "0".repeat(64)),
            ]),
        };
        let plan = prepare(&install, &staged, &manifest, "0.2.3", vec![]).unwrap();
        (root, plan)
    }

    #[test]
    fn interrupted_replacement_rolls_back_and_recovery_is_repeatable() {
        for completed in 0..=4 {
            let (_root, plan) = fixture();
            let prefix = Plan {
                files: plan.files[..completed].to_vec(),
                ..plan.clone()
            };
            apply(&prefix, replace).unwrap();
            assert!(plan.install.join("KarmaService.exe").is_file());
            rollback(&plan, replace).unwrap();
            rollback(&plan, replace).unwrap();
            assert_eq!(
                fs::read(plan.install.join("KarmaService.exe")).unwrap(),
                b"old-service"
            );
            assert!(!plan.install.join("added.dll").exists());
        }
    }

    #[test]
    fn abandoned_preparation_is_removed_before_retry_without_touching_install() {
        let (_root, plan) = fixture();
        cleanup_unjournaled(&plan.install).unwrap();
        cleanup_unjournaled(&plan.install).unwrap();
        assert!(!plan.install.join(BACKUP).exists());
        assert!(!plan.install.join(PREPARED).exists());
        assert_eq!(
            fs::read(plan.install.join("KarmaService.exe")).unwrap(),
            b"old-service"
        );
    }

    #[test]
    fn orphan_applied_transaction_recovers_but_active_health_check_can_boot() {
        assert!(requires_recovery(Phase::Applying, false));
        assert!(requires_recovery(Phase::Applying, true));
        assert!(requires_recovery(Phase::Applied, false));
        assert!(!requires_recovery(Phase::Applied, true));
        assert!(!requires_recovery(Phase::Committed, false));
    }

    #[test]
    fn recovery_after_interruption_during_rollback_restores_old_image() {
        let (_root, plan) = fixture();
        apply(&plan, replace).unwrap();
        let partial = Plan {
            files: plan.files[2..].to_vec(),
            ..plan.clone()
        };
        rollback(&partial, replace).unwrap();
        rollback(&plan, replace).unwrap();
        assert_eq!(
            fs::read(plan.install.join("KarmaService.exe")).unwrap(),
            b"old-service"
        );
        assert!(!plan.install.join("added.dll").exists());
    }

    #[test]
    fn rollback_never_overwrites_files_that_were_not_replaced() {
        fn locked(_: &Path, target: &Path, _: Option<&Path>) -> io::Result<()> {
            Err(io::Error::other(format!("{} is in use", target.display())))
        }
        let (_root, plan) = fixture();
        // Nothing applied: the running image is unchanged and may be locked.
        rollback(&plan, locked).unwrap();
        assert_eq!(
            fs::read(plan.install.join("KarmaService.exe")).unwrap(),
            b"old-service"
        );
        assert!(!plan.install.join(BACKUP).join("KarmaService.exe").exists());
        rollback(&plan, locked).unwrap();
    }

    #[test]
    fn failed_application_restores_all_predecessors() {
        let (_root, plan) = fixture();
        fs::remove_file(plan.install.join(PREPARED).join(SIGNATURE_FILE)).unwrap();
        assert!(apply(&plan, replace).is_err());
        rollback(&plan, replace).unwrap();
        assert_eq!(
            fs::read(plan.install.join("KarmaService.exe")).unwrap(),
            b"old-service"
        );
        assert!(!plan.install.join("added.dll").exists());
    }

    #[test]
    fn committed_cleanup_removes_only_retired_signed_files() {
        let (_root, mut plan) = fixture();
        fs::write(plan.install.join("retired.dll"), "old").unwrap();
        fs::write(plan.install.join("local.config"), "preserve").unwrap();
        plan.retired = vec!["retired.dll".into()];
        apply(&plan, replace).unwrap();
        plan.phase = Phase::Committed;
        cleanup(&plan).unwrap();
        assert!(!plan.install.join("retired.dll").exists());
        assert!(plan.install.join("local.config").exists());
        assert_eq!(
            fs::read(plan.install.join("KarmaService.exe")).unwrap(),
            b"new-service"
        );
    }

    #[test]
    fn a_case_only_windows_rename_is_not_a_retired_file() {
        let manifest = ReleaseManifest {
            format: 1,
            product: "karma".into(),
            version: "0.2.4".into(),
            files: BTreeMap::from([("MODEL/data.onnx".into(), "0".repeat(64))]),
        };
        assert_eq!(
            retired_files(
                vec!["model/Data.onnx".into(), "removed.dll".into()],
                &manifest
            ),
            vec!["removed.dll"]
        );
    }

    #[test]
    fn journal_round_trip_preserves_bounded_transaction() {
        let (root, plan) = fixture();
        let journal = root.path().canonicalize().unwrap().join("plan.json");
        write_plan(&journal, &plan, replace).unwrap();
        let restored = read_plan(&journal).unwrap();
        assert_eq!(restored.version, plan.version);
        assert_eq!(restored.files.len(), 4);
    }
}
