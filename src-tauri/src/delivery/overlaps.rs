//! Files that several open tasks of a batch changed. Each merge of one of
//! them conflicts the others, so the coordinator is shown them to chain
//! those tasks (`builds_on`) or run them one after the other.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde_json::{json, Value};

use super::model::DeliveryOverview;
use super::phase::Phase;
use super::prs::DeliveryPrState;

/// Paths listed per overlap; the rest are only counted.
const FILES_SHOWN: usize = 5;

/// By run id, the groups of open tasks (not merged, not delivered; the
/// overview already leaves out removed and archived ones) that changed the
/// same files, as their jobs recorded them. `None` when there is none.
pub(super) fn task_overlaps(overview: &DeliveryOverview) -> Option<Value> {
    let merged = |task: &str| {
        overview
            .prs
            .iter()
            .any(|pr| pr.task_id == task && pr.state == DeliveryPrState::Merged)
            || overview
                .phases
                .get(task)
                .is_some_and(|phase| phase.phase == Phase::Delivered)
    };
    let open: HashMap<&str, &str> = overview
        .tasks
        .iter()
        .filter(|task| !merged(&task.id))
        .filter_map(|task| Some((task.id.as_str(), task.run_id.as_deref()?)))
        .collect();
    let changes = overview
        .jobs
        .iter()
        .filter(|job| job.undone_at_ms.is_none())
        .filter_map(|job| Some((*open.get(job.task_id.as_str())?, job)))
        .flat_map(|(run, job)| {
            job.changes
                .iter()
                .map(move |change| (run, job.task_id.as_str(), change.path.as_str()))
        });
    let overlaps = overlaps(changes);
    (!overlaps.is_empty()).then(|| json!(overlaps))
}

/// From (run, task, path) changes: per run, each set of two or more tasks
/// with the files exactly they share, most files first.
fn overlaps<'a>(
    changes: impl IntoIterator<Item = (&'a str, &'a str, &'a str)>,
) -> BTreeMap<&'a str, Vec<Value>> {
    let mut by_file: BTreeMap<(&str, &str), BTreeSet<&str>> = BTreeMap::new();
    for (run, task, path) in changes {
        by_file.entry((run, path)).or_default().insert(task);
    }
    let mut by_tasks: BTreeMap<(&str, Vec<&str>), Vec<&str>> = BTreeMap::new();
    for ((run, path), tasks) in by_file {
        if tasks.len() > 1 {
            by_tasks
                .entry((run, tasks.into_iter().collect()))
                .or_default()
                .push(path);
        }
    }
    let mut groups: Vec<_> = by_tasks.into_iter().collect();
    groups.sort_by_key(|(_, files)| std::cmp::Reverse(files.len()));
    let mut runs: BTreeMap<&str, Vec<Value>> = BTreeMap::new();
    for ((run, tasks), files) in groups {
        let mut value = json!({"tasks": tasks, "files": &files[..files.len().min(FILES_SHOWN)]});
        if files.len() > FILES_SHOWN {
            value["more_files"] = json!(files.len() - FILES_SHOWN);
        }
        runs.entry(run).or_default().push(value);
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn groups_shared_files_by_the_tasks_sharing_them_per_run() {
        let mut changes = vec![
            ("r1", "a", "shared.rs"),
            ("r1", "b", "shared.rs"),
            ("r1", "a", "shared.rs"),
            ("r1", "a", "only_a.rs"),
            ("r1", "c", "three.rs"),
            // Another batch's task changing the same path is not an overlap.
            ("r2", "d", "shared.rs"),
        ];
        let many: Vec<String> = (0..7).map(|n| format!("f{n}.rs")).collect();
        for path in &many {
            changes.push(("r1", "a", path));
            changes.push(("r1", "b", path));
            changes.push(("r1", "c", path));
        }
        changes.extend([("r1", "a", "three.rs"), ("r1", "b", "three.rs")]);

        assert_eq!(
            json!(overlaps(changes)),
            json!({"r1": [
                {"tasks": ["a", "b", "c"], "files": ["f0.rs", "f1.rs", "f2.rs", "f3.rs", "f4.rs"], "more_files": 3},
                {"tasks": ["a", "b"], "files": ["shared.rs"]},
            ]})
        );
    }
}
