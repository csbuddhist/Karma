//! Terminate processes that a blocking schedule forbids from running.
//! 终止断网屏蔽时段内被设为"禁止启动"的应用进程。

use std::collections::BTreeSet;

use karma_policy::NetworkRestriction;

#[cfg(windows)]
pub mod windows;

/// Lowercase file names of the executables `restriction` blocks from running,
/// so a process snapshot can skip every other process without opening it.
fn candidate_file_names(restriction: &NetworkRestriction) -> BTreeSet<String> {
    restriction
        .launch_executables
        .iter()
        .filter_map(|executable| executable.rsplit(['\\', '/']).next())
        .filter(|file_name| !file_name.is_empty())
        .map(str::to_lowercase)
        .collect()
}

/// Whether `image` lies inside one of the `protected` directories. Windows
/// components and Karma's own programs are never terminated, whatever the
/// schedule lists.
fn is_protected_image(image: &str, protected: &[String]) -> bool {
    let image = normalize(image);
    protected.iter().any(|directory| {
        let directory = normalize(directory);
        let directory = directory.trim_end_matches('\\');
        !directory.is_empty()
            && image
                .strip_prefix(directory)
                .is_some_and(|rest| rest.starts_with('\\'))
    })
}

fn normalize(path: &str) -> String {
    let path = path.replace('/', "\\").to_lowercase();
    // QueryFullProcessImageNameW may report `\\?\` long-path prefixes.
    match path.strip_prefix(r"\\?\") {
        Some(rest) => rest.to_owned(),
        None => path,
    }
}

/// Whether the process with full image path `image` must be terminated.
fn must_terminate(restriction: &NetworkRestriction, image: &str, protected: &[String]) -> bool {
    restriction.blocks_launch(image) && !is_protected_image(image, protected)
}

#[cfg(test)]
mod tests {
    use karma_policy::NetworkSchedulePolicy;
    use serde_json::json;

    use super::*;

    fn restriction() -> NetworkRestriction {
        NetworkSchedulePolicy::from_value(&json!({
            "applications": [
                {"id":"g","name":"Game","executable":r"D:\Games\Game.exe","category":"game","action":"content_only","enabled":true},
                {"id":"n","name":"Notepad","executable":"notepad.exe","category":"game","action":"content_only","enabled":true},
                {"id":"k","name":"Karma","executable":"karma-ui.exe","category":"game","action":"content_only","enabled":true}
            ],
            "schedules": [{"id":"s", "name":"s", "action":"block_launch", "target":"categories",
                "categories":["game"], "enabled":true,
                "blocks":[{"day":0,"start":"08:00","end":"10:00"}]}]
        }))
        .unwrap()
        .restriction_at(9 * 60)
    }

    fn protected() -> Vec<String> {
        vec![r"C:\Windows".into(), r"C:\Program Files\Karma\".into()]
    }

    #[test]
    fn snapshot_candidates_are_the_blocked_file_names() {
        assert_eq!(
            candidate_file_names(&restriction()),
            BTreeSet::from([
                "game.exe".to_owned(),
                "karma-ui.exe".to_owned(),
                "notepad.exe".to_owned()
            ])
        );
    }

    #[test]
    fn blocked_images_are_terminated_outside_protected_directories() {
        let restriction = restriction();
        assert!(must_terminate(
            &restriction,
            r"d:\games\GAME.EXE",
            &protected()
        ));
        assert!(must_terminate(
            &restriction,
            r"\\?\D:\Tools\notepad.exe",
            &protected()
        ));
        // Category members match by file name, so a moved copy is blocked too.
        assert!(must_terminate(
            &restriction,
            r"E:\Games\Game.exe",
            &protected()
        ));
        assert!(!must_terminate(
            &restriction,
            r"C:\Windows\System32\notepad.exe",
            &protected()
        ));
        assert!(!must_terminate(
            &restriction,
            r"C:\Program Files\Karma\karma-ui.exe",
            &protected()
        ));
        // A sibling directory that shares the prefix is not protected.
        assert!(must_terminate(
            &restriction,
            r"C:\WindowsApps\notepad.exe",
            &protected()
        ));
    }
}
