use std::collections::HashSet;
use std::path::Path;

const MAX: usize = 64;
const SUFFIX_ROOM: usize = 4;

fn tidy(raw: &str) -> Option<String> {
    let joined = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    let clean: String = crate::sanitize::sanitize_text(&joined)
        .chars()
        .take(MAX - SUFFIX_ROOM)
        .collect();
    let clean = clean.trim().to_string();
    (!clean.is_empty()).then_some(clean)
}

pub fn folder_name(cwd: &str) -> Option<String> {
    tidy(&Path::new(cwd).file_name()?.to_string_lossy())
}

pub fn title_name(title: &str) -> Option<String> {
    tidy(title)
}

fn counted(current: &str, base: &str) -> bool {
    current == base
        || current
            .strip_prefix(base)
            .and_then(|rest| rest.strip_prefix(' '))
            .and_then(|n| n.parse::<u32>().ok())
            .is_some_and(|n| n >= 2)
}

pub fn pick(base: &str, current: &str, taken: &HashSet<String>) -> Option<String> {
    if counted(current, base) {
        return None;
    }
    (1u32..)
        .map(|n| {
            if n == 1 {
                base.to_string()
            } else {
                format!("{base} {n}")
            }
        })
        .find(|c| !taken.contains(c))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(names: &[&str]) -> HashSet<String> {
        names.iter().map(|n| n.to_string()).collect()
    }

    #[test]
    fn folder_and_title_names_are_tidy_and_bounded() {
        assert_eq!(
            folder_name("/work/vv-backend").as_deref(),
            Some("vv-backend")
        );
        assert_eq!(folder_name("/"), None);
        assert_eq!(
            title_name("  Fix \n the   build ").as_deref(),
            Some("Fix the build")
        );
        assert_eq!(title_name("   "), None);
        assert!(title_name(&"x".repeat(300)).unwrap().len() <= MAX - SUFFIX_ROOM);
    }

    #[test]
    fn a_taken_name_gets_the_next_free_number() {
        assert_eq!(pick("app", "claude", &set(&[])).as_deref(), Some("app"));
        assert_eq!(
            pick("app", "claude", &set(&["app"])).as_deref(),
            Some("app 2")
        );
        assert_eq!(
            pick("app", "claude", &set(&["app", "app 2"])).as_deref(),
            Some("app 3")
        );
    }

    #[test]
    fn a_name_that_already_fits_is_kept() {
        assert_eq!(pick("app", "app", &set(&["app"])), None);
        assert_eq!(pick("app", "app 3", &set(&["app", "app 3"])), None);
        assert_eq!(pick("app", "app 1", &set(&[])).as_deref(), Some("app"));
        assert_eq!(pick("app", "apple", &set(&[])).as_deref(), Some("app"));
    }
}
