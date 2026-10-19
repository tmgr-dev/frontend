use super::cloud::{
    self, known_private, known_public, mixed, Mode, Outcome, RepoInfo, TouchedBranch,
};
use super::{emit, Opts};
use crate::ctx::Ctx;
use chrono::{SecondsFormat, Utc};
use std::collections::BTreeMap;
use std::path::Path;
use tmgr_core::error::{Error, Kind, Result};
use tmgr_core::text::{sanitize, sanitize_line};
use tmgr_link::privileged;
use tmgr_queue::leak::{
    load_private, load_public, scan, Category, Finding, Index, Location, PrivateSide,
};
use tmgr_queue::state::Held;
use tmgr_queue::workdir;

pub(super) struct Scanned {
    pub applies: bool,
    pub findings: Vec<Finding>,
    pub gated: Vec<String>,
}

pub(super) fn sources_for(
    target: &str,
    names: &[String],
    infos: &BTreeMap<String, Option<RepoInfo>>,
) -> Vec<String> {
    let me = infos.get(target).and_then(|i| i.as_ref());
    names
        .iter()
        .filter(|n| n.as_str() != target)
        .filter(|n| {
            let other = infos.get(n.as_str()).and_then(|i| i.as_ref());
            match (me, other) {
                (_, Some(o)) if known_public(o) => false,
                (Some(m), Some(o))
                    if known_private(m) && known_private(o) && m.owner == o.owner =>
                {
                    false
                }
                _ => true,
            }
        })
        .cloned()
        .collect()
}

pub(super) fn scan_set(
    touched: &[TouchedBranch],
    branch: &str,
    title: &str,
    body: &str,
    lookup: &dyn Fn(&str, &Path) -> Option<RepoInfo>,
) -> Result<Scanned> {
    let names: Vec<String> = touched.iter().map(|t| t.full_name.clone()).collect();
    let cwd = touched.first().map(|t| t.repo.clone()).unwrap_or_default();
    let looked: Vec<Option<RepoInfo>> = names.iter().map(|n| lookup(n, &cwd)).collect();
    let mut scanned = Scanned {
        applies: mixed(&looked),
        findings: Vec::new(),
        gated: Vec::new(),
    };
    if !scanned.applies {
        return Ok(scanned);
    }
    let infos: BTreeMap<String, Option<RepoInfo>> = names.iter().cloned().zip(looked).collect();
    let mut indexes: BTreeMap<Vec<String>, Index> = BTreeMap::new();
    for sink in touched.iter().filter(|t| t.commits > 0) {
        let sources = sources_for(&sink.full_name, &names, &infos);
        if sources.is_empty() {
            continue;
        }
        if !indexes.contains_key(&sources) {
            let mut sides: Vec<PrivateSide> = Vec::new();
            for name in &sources {
                let Some(src) = touched.iter().find(|t| &t.full_name == name) else {
                    continue;
                };
                let mut revs = vec![src.base.clone()];
                if src.commits > 0 {
                    revs.push(src.imported.clone());
                }
                sides.push(load_private(&src.repo, name, &revs)?);
            }
            indexes.insert(sources.clone(), Index::build(&sides));
        }
        let texts = vec![
            ("branch name".to_string(), branch.to_string()),
            ("pull request title".to_string(), title.to_string()),
            ("pull request body".to_string(), body.to_string()),
        ];
        let public = load_public(
            &sink.repo,
            &sink.full_name,
            &sink.base,
            &sink.imported,
            texts,
        )?;
        scanned.findings.extend(scan(&indexes[&sources], &public));
        scanned.gated.push(sink.full_name.clone());
    }
    Ok(scanned)
}

fn categories(findings: &[Finding]) -> Vec<Category> {
    let mut list: Vec<Category> = findings.iter().map(|f| f.category).collect();
    list.sort();
    list.dedup();
    list
}

pub(super) fn held_outcome(scanned: &Scanned, touched: &[TouchedBranch]) -> (Held, String) {
    let cats: Vec<String> = categories(&scanned.findings)
        .iter()
        .map(|c| c.name().to_string())
        .collect();
    let counts: Vec<String> = categories(&scanned.findings)
        .iter()
        .map(|c| {
            let n = scanned.findings.iter().filter(|f| f.category == *c).count();
            format!("{} x{n}", c.name())
        })
        .collect();
    let held = Held {
        at: Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true),
        shas: touched
            .iter()
            .filter(|t| t.commits > 0)
            .map(|t| (t.full_name.clone(), t.imported.clone()))
            .collect(),
        categories: cats,
        findings: scanned.findings.len(),
    };
    let text = format!(
        "the leak gate found {} finding(s) ({}). Nothing was pushed.",
        held.findings,
        counts.join(", ")
    );
    (held, text)
}

pub(super) fn check_pinned(
    expected: &BTreeMap<String, String>,
    touched: &[TouchedBranch],
) -> Result<()> {
    let same = touched
        .iter()
        .filter(|t| t.commits > 0)
        .all(|t| expected.get(&t.full_name) == Some(&t.imported));
    if same {
        Ok(())
    } else {
        Err(Error::new(
            Kind::Conflict,
            "the branch changed since it was held; run --review again",
        )
        .hint("tmgr run <task> --review"))
    }
}

pub(super) fn print_review(scanned: &Scanned, touched: &[TouchedBranch]) {
    if !scanned.applies {
        println!(
            "The leak gate does not apply: the repositories have the same visibility and owner."
        );
        return;
    }
    if scanned.findings.is_empty() {
        println!("The leak gate found nothing.");
    }
    for f in &scanned.findings {
        let place = match &f.location {
            Location::File { path, line } => format!("{}:{line}", sanitize_line(path)),
            Location::Text { label } => sanitize_line(label),
        };
        println!(
            "{} | {} | {} | {} | private: {}",
            f.category.name(),
            sanitize_line(&f.public_repo),
            place,
            sanitize_line(&f.excerpt),
            sanitize_line(f.private_ref.as_deref().unwrap_or("-"))
        );
    }
    for t in touched
        .iter()
        .filter(|t| t.commits > 0 && scanned.gated.contains(&t.full_name))
    {
        let range = format!("{}...{}", t.base, t.imported);
        for (what, flag) in [("diffstat", "--stat"), ("patch", "--patch")] {
            let args = [
                "-c",
                "core.quotepath=off",
                "diff",
                "--no-ext-diff",
                "--no-textconv",
                "--no-color",
                "--no-renames",
                flag,
                "--end-of-options",
                &range,
            ];
            match workdir::git_query(&t.repo, &args) {
                Ok(out) => println!(
                    "--- {} {what} ---\n{}",
                    sanitize_line(&t.full_name),
                    sanitize(&out)
                ),
                Err(e) => eprintln!(
                    "warning: could not show the {what}: {}",
                    sanitize_line(&e.message)
                ),
            }
        }
    }
}

pub(super) fn confirm_push(scanned: &Scanned, touched: &[TouchedBranch]) -> Result<()> {
    let names: Vec<&str> = if scanned.gated.is_empty() {
        touched
            .iter()
            .filter(|t| t.commits > 0)
            .map(|t| t.full_name.as_str())
            .collect()
    } else {
        scanned.gated.iter().map(String::as_str).collect()
    };
    let typed = names.join(" ");
    privileged::confirm_typed_stdio(&typed, &format!("Type {typed} to push the held branch:"))
}

pub fn review(o: &Opts, reference: &str) -> Result<i32> {
    privileged::guard()?;
    let loc = cloud::locate(o, reference)?;
    if let Some(h) = &loc.held {
        println!(
            "Held since {} ({}).",
            sanitize_line(&h.at),
            sanitize_line(&h.categories.join(", "))
        );
    }
    cloud::publish_outcome(o, &loc.target, Mode::Review)?;
    Ok(0)
}

pub fn push(o: &Opts, reference: &str) -> Result<i32> {
    privileged::guard()?;
    let loc = cloud::locate(o, reference)?;
    let held = loc.held.clone().ok_or_else(|| {
        Error::conflict("nothing is held for this task").hint("tmgr run <task> --publish")
    })?;
    match cloud::publish_outcome(o, &loc.target, Mode::Push(held.shas))? {
        Outcome::Published(prs) => {
            cloud::clear_held(&loc, prs.first().map(|(_, u)| u.as_str()))?;
            Ok(cloud::emit_prs(o, &prs))
        }
        _ => Err(Error::internal("the push did not publish")),
    }
}

pub fn drop_held(o: &Opts, reference: &str) -> Result<i32> {
    privileged::guard()?;
    let loc = cloud::locate(o, reference)?;
    if loc.held.is_none() {
        return Err(Error::conflict("nothing is held for this task"));
    }
    privileged::confirm_typed_stdio(
        &loc.task_key,
        &format!(
            "Type {} to drop the held branch:",
            sanitize_line(&loc.task_key)
        ),
    )?;
    cloud::release_held(&loc)?;
    let ctx = Ctx::new(o.cli)?;
    let client = ctx.cloud_client()?;
    if let Err(e) = client.add_comment(
        u64::try_from(loc.target.task_id).unwrap_or_default(),
        "Dropped by the owner after the leak gate; nothing was pushed.",
    ) {
        eprintln!(
            "warning: could not add the comment: {}",
            sanitize_line(&e.message)
        );
    }
    Ok(emit(
        o,
        serde_json::json!({ "dropped": true }),
        format!(
            "dropped; the run folder {} is left in place",
            sanitize_line(&loc.target.clone_dir.display().to_string())
        ),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(visibility: &str, owner: &str) -> Option<RepoInfo> {
        Some(RepoInfo {
            visibility: visibility.into(),
            owner: owner.into(),
        })
    }

    fn pick(target: &str, repos: &[(&str, Option<RepoInfo>)]) -> Vec<String> {
        let names: Vec<String> = repos.iter().map(|(n, _)| n.to_string()).collect();
        let infos = repos
            .iter()
            .map(|(n, i)| (n.to_string(), i.clone()))
            .collect();
        sources_for(target, &names, &infos)
    }

    #[test]
    fn a_public_repository_is_never_a_source() {
        let repos = [("o/a", info("public", "o")), ("o/b", info("public", "o"))];
        assert!(pick("o/a", &repos).is_empty());
        let repos = [("o/a", info("private", "o")), ("o/b", info("public", "o"))];
        assert!(pick("o/a", &repos).is_empty());
    }

    #[test]
    fn a_private_repository_is_a_source_for_a_public_one() {
        let repos = [("o/a", info("public", "o")), ("o/b", info("private", "o"))];
        assert_eq!(pick("o/a", &repos), ["o/b"]);
        let repos = [("o/a", info("public", "o")), ("o/b", info("internal", "o"))];
        assert_eq!(pick("o/a", &repos), ["o/b"]);
    }

    #[test]
    fn private_repositories_of_one_owner_trust_each_other_but_not_other_owners() {
        let same = [("o/a", info("private", "o")), ("o/b", info("private", "o"))];
        assert!(pick("o/a", &same).is_empty());
        let other = [("o/a", info("private", "o")), ("p/b", info("private", "p"))];
        assert_eq!(pick("o/a", &other), ["p/b"]);
    }

    #[test]
    fn an_unknown_repository_counts_on_both_sides() {
        let repos = [("o/a", None), ("o/b", info("private", "o"))];
        assert_eq!(pick("o/a", &repos), ["o/b"]);
        assert!(pick("o/b", &repos).len() == 1);
        let repos = [("o/a", None), ("o/b", None)];
        assert_eq!(pick("o/a", &repos), ["o/b"]);
        let repos = [("o/a", None), ("o/b", info("public", "o"))];
        assert!(pick("o/a", &repos).is_empty());
        let repos = [("o/a", info("private", "o")), ("o/b", None)];
        assert_eq!(pick("o/a", &repos), ["o/b"]);
    }

    #[test]
    fn a_branch_that_moved_is_not_the_held_one() {
        let t = |sha: &str| TouchedBranch {
            repo: Default::default(),
            before: None,
            imported: sha.into(),
            full_name: "o/a".into(),
            base: String::new(),
            commits: 1,
        };
        let held: BTreeMap<String, String> = [("o/a".to_string(), "aaa".to_string())].into();
        assert!(check_pinned(&held, &[t("aaa")]).is_ok());
        assert!(check_pinned(&held, &[t("bbb")]).is_err());
        assert!(check_pinned(&BTreeMap::new(), &[t("aaa")]).is_err());
    }
}
