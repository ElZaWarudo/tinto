use super::*;

fn injected_failure() -> io::Error {
    io::Error::other("fallo inyectado")
}

fn assert_no_transaction_artifacts(parent: &Path) {
    let artifacts: Vec<PathBuf> = fs::read_dir(parent)
        .expect("read transaction parent")
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.file_name()
                .is_some_and(|name| name.to_string_lossy().contains(".tinto-"))
        })
        .collect();
    assert!(
        artifacts.is_empty(),
        "transaction artifacts remained: {artifacts:?}"
    );
}

fn snapshot_tree(root: &Path) -> Vec<(PathBuf, Option<Vec<u8>>)> {
    fn visit(root: &Path, path: &Path, snapshot: &mut Vec<(PathBuf, Option<Vec<u8>>)>) {
        let relative = path.strip_prefix(root).expect("snapshot path inside root");
        if path.is_dir() {
            if !relative.as_os_str().is_empty() {
                snapshot.push((relative.to_path_buf(), None));
            }
            let mut children: Vec<PathBuf> = fs::read_dir(path)
                .expect("read snapshot directory")
                .map(|entry| entry.expect("snapshot entry").path())
                .collect();
            children.sort();
            for child in children {
                visit(root, &child, snapshot);
            }
        } else {
            snapshot.push((
                relative.to_path_buf(),
                Some(fs::read(path).expect("read snapshot file")),
            ));
        }
    }

    let mut snapshot = Vec::new();
    visit(root, root, &mut snapshot);
    snapshot
}

#[test]
fn delete_requires_explicit_user_consent() {
    let error = require_delete_user_consent(false).unwrap_err();

    assert_eq!(error.category, "user-consent-required");
    assert!(error.message.contains("confirmación explícita"));
    assert!(require_delete_user_consent(true).is_ok());
}

#[test]
fn copy_overwrite_replaces_a_directory_only_after_staging_completes() {
    let temp = tempfile::tempdir().expect("temp dir");
    let src = temp.path().join("source");
    let dest = temp.path().join("destination");
    fs::create_dir_all(&src).expect("source dir");
    fs::create_dir_all(&dest).expect("destination dir");
    fs::write(src.join("new.txt"), "new").expect("source file");
    fs::write(dest.join("old.txt"), "old").expect("destination file");

    transactional_copy(&src, &dest).expect("transactional copy");

    assert_eq!(fs::read_to_string(src.join("new.txt")).unwrap(), "new");
    assert_eq!(fs::read_to_string(dest.join("new.txt")).unwrap(), "new");
    assert!(!dest.join("old.txt").exists());
    assert_no_transaction_artifacts(temp.path());
}

#[test]
fn copy_overwrite_keeps_the_original_destination_when_staging_is_rejected() {
    let temp = tempfile::tempdir().expect("temp dir");
    let src = temp.path().join("source.txt");
    let dest = temp.path().join("destination.txt");
    fs::write(&src, "new").expect("source");
    fs::write(&dest, "old").expect("destination");

    let result = transactional_copy_with_hook(&src, &dest, |phase| {
        if phase == ReplacementPhase::Staged {
            Err(injected_failure())
        } else {
            Ok(())
        }
    });

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&src).unwrap(), "new");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "old");
    assert_no_transaction_artifacts(temp.path());
}

#[test]
fn copy_overwrite_cleans_a_partially_copied_file_without_touching_either_end() {
    let temp = tempfile::tempdir().expect("temp dir");
    let src = temp.path().join("source.bin");
    let dest = temp.path().join("destination.bin");
    let source_before: Vec<u8> = (0..=255).cycle().take(8 * 1024).collect();
    let destination_before: Vec<u8> = (0..=127).rev().cycle().take(4 * 1024).collect();
    fs::write(&src, &source_before).expect("source");
    fs::write(&dest, &destination_before).expect("destination");

    let result = transactional_copy_with_stage_copy(&src, &dest, |from, stage| {
        let bytes = fs::read(from)?;
        fs::write(stage, &bytes[..bytes.len() / 2])?;
        Err(injected_failure())
    });

    assert!(result.is_err());
    assert_eq!(fs::read(&src).unwrap(), source_before);
    assert_eq!(fs::read(&dest).unwrap(), destination_before);
    assert_no_transaction_artifacts(temp.path());
}

#[test]
fn move_overwrite_cleans_a_partially_copied_directory_without_touching_either_end() {
    let temp = tempfile::tempdir().expect("temp dir");
    let src = temp.path().join("source");
    let dest = temp.path().join("destination");
    fs::create_dir_all(src.join("nested")).expect("source dirs");
    fs::create_dir_all(dest.join("kept")).expect("destination dirs");
    fs::write(src.join("first.bin"), vec![0xA5; 4096]).expect("first source file");
    fs::write(src.join("nested/second.bin"), vec![0x5A; 8192]).expect("second source file");
    fs::write(dest.join("old.bin"), vec![0x11; 1024]).expect("old destination file");
    fs::write(dest.join("kept/data.bin"), vec![0x22; 2048]).expect("nested destination file");
    let source_before = snapshot_tree(&src);
    let destination_before = snapshot_tree(&dest);

    let result = transactional_move_with_stage_copy(&src, &dest, |from, stage| {
        fs::create_dir_all(stage.join("nested"))?;
        fs::copy(from.join("first.bin"), stage.join("first.bin"))?;
        let second = fs::read(from.join("nested/second.bin"))?;
        fs::write(stage.join("nested/second.bin"), &second[..second.len() / 2])?;
        Err(injected_failure())
    });

    assert!(result.is_err());
    assert_eq!(snapshot_tree(&src), source_before);
    assert_eq!(snapshot_tree(&dest), destination_before);
    assert_no_transaction_artifacts(temp.path());
}

#[test]
fn copy_overwrite_rolls_back_after_the_destination_is_backed_up() {
    let temp = tempfile::tempdir().expect("temp dir");
    let src = temp.path().join("source.txt");
    let dest = temp.path().join("destination.txt");
    fs::write(&src, "new").expect("source");
    fs::write(&dest, "old").expect("destination");

    let result = transactional_copy_with_hook(&src, &dest, |phase| {
        if phase == ReplacementPhase::DestinationBackedUp {
            Err(injected_failure())
        } else {
            Ok(())
        }
    });

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&src).unwrap(), "new");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "old");
    assert_no_transaction_artifacts(temp.path());
}

#[test]
fn move_overwrite_rolls_back_both_paths_after_the_source_is_backed_up() {
    let temp = tempfile::tempdir().expect("temp dir");
    let src = temp.path().join("source.txt");
    let dest = temp.path().join("destination.txt");
    fs::write(&src, "new").expect("source");
    fs::write(&dest, "old").expect("destination");

    let result = transactional_move_with_hook(&src, &dest, |phase| {
        if phase == ReplacementPhase::SourceBackedUp {
            Err(injected_failure())
        } else {
            Ok(())
        }
    });

    assert!(result.is_err());
    assert_eq!(fs::read_to_string(&src).unwrap(), "new");
    assert_eq!(fs::read_to_string(&dest).unwrap(), "old");
    assert_no_transaction_artifacts(temp.path());
}

#[test]
fn move_overwrite_commits_and_removes_transaction_artifacts() {
    let temp = tempfile::tempdir().expect("temp dir");
    let src = temp.path().join("source.txt");
    let dest = temp.path().join("destination.txt");
    fs::write(&src, "new").expect("source");
    fs::write(&dest, "old").expect("destination");

    transactional_move(&src, &dest).expect("transactional move");

    assert!(!src.exists());
    assert_eq!(fs::read_to_string(&dest).unwrap(), "new");
    assert_no_transaction_artifacts(temp.path());
}

#[test]
fn copy_batch_rolls_back_the_first_object_when_the_next_step_fails() {
    let temp = tempfile::tempdir().expect("temp dir");
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(&source).expect("source dir");
    fs::create_dir_all(&destination).expect("destination dir");
    fs::write(source.join("one.bin"), vec![0x11; 4096]).expect("source one");
    fs::write(source.join("two.bin"), vec![0x22; 8192]).expect("source two");
    fs::write(destination.join("one.bin"), vec![0xA1; 1024]).expect("destination one");
    fs::write(destination.join("two.bin"), vec![0xA2; 2048]).expect("destination two");
    let before = snapshot_tree(temp.path());

    let result = run_copy_batch_with_hook(
        vec![
            (source.join("one.bin"), destination.join("one.bin")),
            (source.join("two.bin"), destination.join("two.bin")),
        ],
        |completed| {
            if completed == 1 {
                Err(injected_failure())
            } else {
                Ok(())
            }
        },
    );

    assert!(result.is_err());
    assert_eq!(snapshot_tree(temp.path()), before);
    assert_no_transaction_artifacts(&source);
    assert_no_transaction_artifacts(&destination);
}

#[test]
fn move_batch_rolls_back_the_first_object_when_the_next_step_fails() {
    let temp = tempfile::tempdir().expect("temp dir");
    let source = temp.path().join("source");
    let destination = temp.path().join("destination");
    fs::create_dir_all(&source).expect("source dir");
    fs::create_dir_all(&destination).expect("destination dir");
    fs::write(source.join("one.bin"), vec![0x31; 4096]).expect("source one");
    fs::write(source.join("two.bin"), vec![0x32; 8192]).expect("source two");
    fs::write(destination.join("one.bin"), vec![0xB1; 1024]).expect("destination one");
    fs::write(destination.join("two.bin"), vec![0xB2; 2048]).expect("destination two");
    let before = snapshot_tree(temp.path());

    let result = run_move_batch_with_hook(
        vec![
            (source.join("one.bin"), destination.join("one.bin")),
            (source.join("two.bin"), destination.join("two.bin")),
        ],
        |completed| {
            if completed == 1 {
                Err(injected_failure())
            } else {
                Ok(())
            }
        },
    );

    assert!(result.is_err());
    assert_eq!(snapshot_tree(temp.path()), before);
    assert_no_transaction_artifacts(&source);
    assert_no_transaction_artifacts(&destination);
}

#[test]
fn delete_batch_restores_every_source_and_discards_its_failed_token() {
    let temp = tempfile::tempdir().expect("temp dir");
    let repo = temp.path().join("repo");
    let backup_root = temp.path().join("undo-token");
    fs::create_dir_all(&repo).expect("repo dir");
    fs::create_dir_all(&backup_root).expect("backup root");
    fs::write(backup_root.join("manifest.json"), b"manifest").expect("manifest");
    fs::write(repo.join("one.bin"), vec![0x41; 4096]).expect("source one");
    fs::write(repo.join("two.bin"), vec![0x42; 8192]).expect("source two");
    let before = snapshot_tree(&repo);
    let objects = backup_root.join("objects");

    let result = run_delete_batch_with_hook(
        &backup_root,
        vec![
            (repo.join("one.bin"), objects.join("0")),
            (repo.join("two.bin"), objects.join("1")),
        ],
        |completed| {
            if completed == 1 {
                Err(injected_failure())
            } else {
                Ok(())
            }
        },
    );

    assert!(result.is_err());
    assert_eq!(snapshot_tree(&repo), before);
    assert!(!backup_root.exists());
    assert_no_transaction_artifacts(&repo);
}

#[test]
fn restore_batch_keeps_the_token_retriable_after_a_failure() {
    let temp = tempfile::tempdir().expect("temp dir");
    let repo = temp.path().join("repo");
    let backup_root = temp.path().join("undo-token");
    let objects = backup_root.join("objects");
    fs::create_dir_all(&repo).expect("repo dir");
    fs::create_dir_all(&objects).expect("objects dir");
    fs::write(repo.join("kept.bin"), vec![0x50; 512]).expect("repo sentinel");
    fs::write(objects.join("0"), vec![0x51; 4096]).expect("backup one");
    fs::write(objects.join("1"), vec![0x52; 8192]).expect("backup two");
    let before = snapshot_tree(temp.path());

    let result = run_replay_batch_with_hook(
        "retry-token",
        vec![
            (objects.join("0"), repo.join("nested/one.bin")),
            (objects.join("1"), repo.join("nested/two.bin")),
        ],
        true,
        |completed| {
            if completed == 1 {
                Err(injected_failure())
            } else {
                Ok(())
            }
        },
    );

    assert!(result.is_err());
    assert_eq!(snapshot_tree(temp.path()), before);
    assert_no_transaction_artifacts(&objects);
    assert_no_transaction_artifacts(&repo);
}

#[test]
fn redo_batch_keeps_the_token_retriable_after_a_failure() {
    let temp = tempfile::tempdir().expect("temp dir");
    let repo = temp.path().join("repo");
    let backup_root = temp.path().join("undo-token");
    let objects = backup_root.join("objects");
    fs::create_dir_all(&repo).expect("repo dir");
    fs::create_dir_all(&objects).expect("objects dir");
    fs::write(repo.join("one.bin"), vec![0x61; 4096]).expect("source one");
    fs::write(repo.join("two.bin"), vec![0x62; 8192]).expect("source two");
    let before = snapshot_tree(temp.path());

    let result = run_replay_batch_with_hook(
        "retry-token",
        vec![
            (repo.join("one.bin"), objects.join("0")),
            (repo.join("two.bin"), objects.join("1")),
        ],
        false,
        |completed| {
            if completed == 1 {
                Err(injected_failure())
            } else {
                Ok(())
            }
        },
    );

    assert!(result.is_err());
    assert_eq!(snapshot_tree(temp.path()), before);
    assert_no_transaction_artifacts(&objects);
    assert_no_transaction_artifacts(&repo);
}

#[test]
fn cleanup_failure_is_returned_as_an_explicit_non_fatal_warning() {
    let path = PathBuf::from("stale.tinto-backup");

    let warning =
        cleanup_after_commit_with(&path, |_| Err(injected_failure())).expect("cleanup warning");

    assert!(warning.contains("La operación terminó"));
    assert!(warning.contains("stale.tinto-backup"));
    assert!(warning.contains("fallo inyectado"));
}

#[test]
fn failed_source_restore_keeps_the_installed_copy_recoverable() {
    let temp = tempfile::tempdir().expect("temp dir");
    let source = temp.path().join("source.txt");
    let backup = temp.path().join("objects/0");
    fs::create_dir_all(backup.parent().unwrap()).expect("objects");
    fs::write(&source, "recoverable").expect("source");
    let mut hook = |_| Ok(());
    let mut stage_copy = copy_recursive;
    let installed =
        install_move(&source, &backup, &mut hook, &mut stage_copy).expect("installed delete move");

    let error = installed
        .rollback_with_source_restore(|_, _| Err(injected_failure()))
        .unwrap_err();

    assert!(error.to_string().contains("se conservó una copia"));
    assert!(!source.exists());
    assert_eq!(fs::read_to_string(&backup).unwrap(), "recoverable");
    assert!(recovery_manifest_is_usable(&[(source, backup)]));
}

#[test]
fn recovery_token_is_bound_to_the_repo_that_created_it() {
    let temp = tempfile::tempdir().expect("temp dir");
    let repo = temp.path().join("repo");
    let other_repo = temp.path().join("other");
    fs::create_dir_all(&repo).expect("repo");
    fs::create_dir_all(&other_repo).expect("other repo");
    let repo = repo.canonicalize().expect("canonical repo");
    let token = Uuid::new_v4().to_string();
    let backup_root = undo_backup_root(&token).expect("backup root");
    write_delete_manifest(
        &backup_root,
        &DeleteManifest {
            token: token.clone(),
            repo: repo.clone(),
            entries: Vec::new(),
        },
    )
    .expect("manifest");

    assert!(read_bound_delete_manifest(&repo, &token).is_ok());
    let error = read_bound_delete_manifest(&other_repo, &token).unwrap_err();
    assert_eq!(error.category, "undo-repo-mismatch");

    remove_path(&backup_root).expect("cleanup token");
}

#[test]
fn partial_recovery_manifest_restores_only_missing_entries_and_remains_redoable() {
    let temp = tempfile::tempdir().expect("temp dir");
    let repo = temp.path().join("repo");
    fs::create_dir_all(&repo).expect("repo");
    fs::write(repo.join("already.txt"), "already restored").expect("existing entry");
    let repo = repo.canonicalize().expect("canonical repo");
    let token = Uuid::new_v4().to_string();
    let backup_root = undo_backup_root(&token).expect("backup root");
    let objects = backup_root.join("objects");
    fs::create_dir_all(&objects).expect("objects");
    fs::write(objects.join("1"), "recover me").expect("recoverable object");
    let manifest = DeleteManifest {
        token: token.clone(),
        repo: repo.clone(),
        entries: vec![
            DeletedEntry {
                path: PathBuf::from("already.txt"),
                is_dir: false,
                backup_name: "0".into(),
            },
            DeletedEntry {
                path: PathBuf::from("missing.txt"),
                is_dir: false,
                backup_name: "1".into(),
            },
        ],
    };
    write_delete_manifest(&backup_root, &manifest).expect("manifest");

    let restore =
        plan_delete_replay(&repo, &manifest, ReplayDirection::Restore).expect("restore plan");
    assert_eq!(restore.len(), 1);
    assert!(
        run_replay_batch_with_hook(&token, restore, true, |_| Ok(()))
            .expect("restore")
            .is_empty()
    );
    assert_eq!(
        fs::read_to_string(repo.join("already.txt")).unwrap(),
        "already restored"
    );
    assert_eq!(
        fs::read_to_string(repo.join("missing.txt")).unwrap(),
        "recover me"
    );

    let redo = plan_delete_replay(&repo, &manifest, ReplayDirection::Redo).expect("redo plan");
    assert_eq!(redo.len(), 2);
    run_replay_batch_with_hook(&token, redo, false, |_| Ok(())).expect("redo");
    assert!(objects.join("0").exists());
    assert!(objects.join("1").exists());

    remove_path(&backup_root).expect("cleanup token");
}
