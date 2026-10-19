use serde::{Deserialize, Serialize};
use tmgr_core::approval::canonical_repos;
use tmgr_core::error::{Error, Kind, Result};

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimRepo {
    pub full_name: String,
    pub path: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClonePlan {
    pub full_name: String,
    pub name: String,
    pub path: String,
}

fn valid_clone_name(name: &str) -> bool {
    let lower_alnum = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit();
    (1..=100).contains(&name.len())
        && name.chars().next().is_some_and(lower_alnum)
        && name
            .chars()
            .all(|c| lower_alnum(c) || matches!(c, '.' | '_' | '-'))
        && name != ".git"
}

/// Folder name of each repository inside a run folder, in canonical (sorted) order of the full names: the
/// lowercased repo part, or `<owner>-<repo>` for the names whose repo part is shared. Refuses a name that
/// is not `^[a-z0-9][a-z0-9._-]{0,99}$`, is `.git`, or collides with another one.
pub fn derive_names(full_names: &[String]) -> std::result::Result<Vec<(String, String)>, String> {
    let sorted = canonical_repos(full_names)?;
    let split = |n: &str| {
        let (owner, repo) = n.split_once('/').unwrap_or((n, ""));
        (owner.to_ascii_lowercase(), repo.to_ascii_lowercase())
    };
    let mut out: Vec<(String, String)> = Vec::new();
    for full in &sorted {
        let (owner, repo) = split(full);
        let shared = sorted.iter().filter(|o| split(o).1 == repo).count() > 1;
        let name = if shared {
            format!("{owner}-{repo}")
        } else {
            repo
        };
        if !valid_clone_name(&name) {
            return Err(format!(
                "{full} has no usable folder name ({name:?} is not a-z, 0-9, . _ - of at most 100 characters)"
            ));
        }
        if out.iter().any(|(_, n)| *n == name) {
            return Err(format!(
                "two repositories would share the folder name {name}"
            ));
        }
        out.push((full.clone(), name));
    }
    Ok(out)
}

/// The clones of a multi-repository run, primary (the full name that sorts first) first.
pub fn plan_clones(repos: &[ClaimRepo]) -> Result<Vec<ClonePlan>> {
    let names: Vec<String> = repos.iter().map(|r| r.full_name.clone()).collect();
    let derived = derive_names(&names).map_err(|m| Error::new(Kind::Forbidden, m))?;
    derived
        .into_iter()
        .map(|(full_name, name)| {
            let path = repos
                .iter()
                .find(|r| r.full_name == full_name)
                .map(|r| r.path.clone())
                .ok_or_else(|| Error::internal("repository list changed while planning"))?;
            Ok(ClonePlan {
                full_name,
                name,
                path,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn names(list: &[&str]) -> std::result::Result<Vec<(String, String)>, String> {
        derive_names(&list.iter().map(|s| s.to_string()).collect::<Vec<_>>())
    }

    fn pairs(list: &[(&str, &str)]) -> Vec<(String, String)> {
        list.iter()
            .map(|(a, b)| (a.to_string(), b.to_string()))
            .collect()
    }

    #[test]
    fn the_repo_part_names_the_folder_and_the_primary_sorts_first() {
        assert_eq!(
            names(&["tmgr-dev/frontend", "tmgr-dev/backend"]).unwrap(),
            pairs(&[
                ("tmgr-dev/backend", "backend"),
                ("tmgr-dev/frontend", "frontend")
            ])
        );
        assert_eq!(
            names(&["tmgr-dev/backend"]).unwrap(),
            pairs(&[("tmgr-dev/backend", "backend")])
        );
    }

    #[test]
    fn only_the_names_sharing_a_repo_part_get_the_owner_prefix() {
        assert_eq!(
            names(&["b/web", "a/web", "a/api"]).unwrap(),
            pairs(&[("a/api", "api"), ("a/web", "a-web"), ("b/web", "b-web")])
        );
    }

    #[test]
    fn unusable_and_colliding_names_are_refused() {
        assert!(names(&["acme/.github"]).is_err());
        assert!(names(&["acme/.hidden", "other/api"]).is_err());
        assert!(names(&["a-b/c", "a/b-c", "x/c", "y/b-c"]).is_err());
        let long_owner = "o".repeat(39);
        let long_repo = "r".repeat(100);
        assert!(names(&[
            &format!("{long_owner}/{long_repo}"),
            &format!("p/{long_repo}")
        ])
        .is_err());
        assert!(names(&["Acme/Web"]).is_err());
        assert!(names(&[]).is_err());
        assert!(names(&["a/x", "a/x"]).is_err());
    }

    #[test]
    fn plans_keep_each_path_with_its_repository() {
        let repos = vec![
            ClaimRepo {
                full_name: "tmgr-dev/frontend".into(),
                path: "/w/fe".into(),
            },
            ClaimRepo {
                full_name: "tmgr-dev/backend".into(),
                path: "/w/be".into(),
            },
        ];
        let plans = plan_clones(&repos).unwrap();
        assert_eq!(plans[0].name, "backend");
        assert_eq!(plans[0].path, "/w/be");
        assert_eq!(plans[1].name, "frontend");
        assert_eq!(plans[1].path, "/w/fe");
    }
}
