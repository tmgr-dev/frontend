use base64::Engine;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Instant;
use tmgr_queue::leak::*;

fn git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(args)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }
}

fn rename_idents(text: &str) -> String {
    let mut out = String::new();
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        out.push_str(word);
        if word
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic() || c == '_')
        {
            out.push_str("Zz");
        }
        word.clear();
    };
    for c in text.chars() {
        if c.is_alphanumeric() || c == '_' {
            word.push(c);
        } else {
            flush(&mut word, &mut out);
            out.push(c);
        }
    }
    flush(&mut word, &mut out);
    out
}

fn resplit(text: &str) -> String {
    let words: Vec<&str> = text.split_whitespace().collect();
    words
        .chunks(4)
        .map(|c| format!("{}\n", c.join(" ")))
        .collect()
}

fn positive_control(private_repo: &Path, private: &PrivateSide, index: &Index) {
    let mut rng = Rng(0x2545_f491_4f6c_dd1d);
    let code_ext = [
        "java", "kt", "ts", "js", "vue", "php", "py", "rs", "go", "sql",
    ];
    let candidates: Vec<&SourceFile> = private
        .files
        .iter()
        .filter(|f| {
            let ext = f.path.rsplit('.').next().unwrap_or("");
            code_ext.contains(&ext) && f.text.lines().count() >= 30
        })
        .collect();
    let _ = private_repo;
    let mut rates: BTreeMap<&str, (usize, usize)> = BTreeMap::new();
    let mut tried = 0;
    while tried < 30 && !candidates.is_empty() {
        let f = candidates[(rng.next() as usize) % candidates.len()];
        let lines: Vec<&str> = f.text.lines().collect();
        let mut chunk: Option<String> = None;
        for _ in 0..20 {
            let start = (rng.next() as usize) % (lines.len() - 14);
            let slice = &lines[start..start + 15];
            if slice
                .iter()
                .filter(|l| {
                    let t = l.trim();
                    t.chars().filter(|c| c.is_alphanumeric()).count() >= 4
                        && !t.starts_with("import ")
                        && !t.starts_with("package ")
                        && !t.starts_with("use ")
                })
                .count()
                >= 12
            {
                chunk = Some(slice.join("\n"));
                break;
            }
        }
        let Some(chunk) = chunk else { continue };
        tried += 1;
        let b64 = base64::engine::general_purpose::STANDARD.encode(&chunk);
        let wrapped: String = b64
            .as_bytes()
            .chunks(76)
            .map(|c| format!("{}\n", String::from_utf8_lossy(c)))
            .collect();
        let variants: [(&str, String); 6] = [
            (
                "verbatim 6 lines",
                chunk.lines().take(6).collect::<Vec<_>>().join("\n"),
            ),
            ("verbatim", chunk.clone()),
            ("renamed identifiers", rename_idents(&chunk)),
            ("lines re-split", resplit(&chunk)),
            ("comment-wrapped", format!("/*\n{chunk}\n*/")),
            ("base64", wrapped),
        ];
        for (name, text) in variants {
            let public = PublicSide {
                full_name: "pub/ctl".into(),
                base_files: vec![],
                new_files: vec![],
                added: vec![AddedBlock {
                    path: "ctl.ts".into(),
                    line: 1,
                    text,
                }],
                texts: vec![],
                unscannable: vec![],
            };
            let hit = scan(index, &public)
                .iter()
                .any(|x| x.category == Category::Overlap);
            let e = rates.entry(name).or_default();
            e.0 += hit as usize;
            e.1 += 1;
        }
    }
    println!("positive control ({tried} private files, 15-line chunks):");
    for (name, (hit, total)) in rates {
        println!("  {name}: {hit}/{total}");
    }
}

#[test]
#[ignore]
fn measure() {
    let private_repo =
        PathBuf::from(std::env::var("LEAK_PRIVATE_REPO").expect("LEAK_PRIVATE_REPO"));
    let public_repo = PathBuf::from(std::env::var("LEAK_PUBLIC_REPO").expect("LEAK_PUBLIC_REPO"));
    let n: usize = std::env::var("LEAK_N")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(300);
    let started = Instant::now();
    let commits: Vec<String> = git(
        &public_repo,
        &[
            "rev-list",
            "--first-parent",
            "--no-merges",
            "-n",
            &n.to_string(),
            "origin/master",
        ],
    )
    .lines()
    .map(str::to_string)
    .collect();

    let mut cached: Option<(String, Index)> = None;
    let mut flagged: Vec<(String, Vec<Finding>)> = Vec::new();
    let mut per_cat: BTreeMap<Category, usize> = BTreeMap::new();
    let mut scanned = 0;
    for c in &commits {
        let parents = git(&public_repo, &["rev-list", "--parents", "-n", "1", c]);
        let Some(parent) = parents.split(' ').nth(1) else {
            continue;
        };
        let ts = git(&public_repo, &["show", "-s", "--format=%ct", c]);
        let until = format!("--until={ts}");
        let psha = git(
            &private_repo,
            &["rev-list", "--first-parent", "-1", &until, "origin/master"],
        );
        if psha.is_empty() {
            continue;
        }
        if cached.as_ref().map(|c| c.0.as_str()) != Some(psha.as_str()) {
            let side =
                load_private(&private_repo, "acme/backend", std::slice::from_ref(&psha)).unwrap();
            cached = Some((psha.clone(), Index::build(&[side])));
        }
        let index = &cached.as_ref().unwrap().1;
        let public = load_public(&public_repo, "acme/frontend", parent, c, vec![]).unwrap();
        let findings = scan(index, &public);
        scanned += 1;
        if !findings.is_empty() {
            let mut seen = std::collections::BTreeSet::new();
            for f in &findings {
                seen.insert(f.category);
            }
            for cat in seen {
                *per_cat.entry(cat).or_default() += 1;
            }
            flagged.push((c[..7].to_string(), findings));
        }
    }
    println!("scanned {scanned} commits, flagged {}", flagged.len());
    println!(
        "flagged / N: {:.1}%",
        100.0 * flagged.len() as f64 / scanned.max(1) as f64
    );
    for (cat, count) in &per_cat {
        println!("  {}: {count}", cat.name());
    }
    for (sha, findings) in &flagged {
        println!("{sha}");
        for f in findings {
            let loc = match &f.location {
                Location::File { path, line } => format!("{path}:{line}"),
                Location::Text { label } => label.clone(),
            };
            let pref = f.private_ref.clone().unwrap_or_default();
            println!("  [{}] {loc}  <- {pref}", f.category.name());
        }
    }
    println!("history scan took {:.1}s", started.elapsed().as_secs_f64());

    let newest = git(&private_repo, &["rev-parse", "origin/master"]);
    let side = load_private(&private_repo, "acme/backend", &[newest]).unwrap();
    let index = Index::build(std::slice::from_ref(&side));
    positive_control(&private_repo, &side, &index);
    println!("total {:.1}s", started.elapsed().as_secs_f64());
}
