use base64::Engine;
use std::path::Path;
use std::process::Command;
use tmgr_queue::leak::*;

fn code(tag: &str, n: usize) -> String {
    (0..n)
        .map(|i| {
            format!(
                "        total{tag}{i} += ledger.apply{tag}(entry{tag}{i}, rate{i} * {});\n",
                i + 3
            )
        })
        .collect()
}

fn file(path: &str, text: &str) -> SourceFile {
    SourceFile {
        path: path.into(),
        text: text.into(),
    }
}

fn private(files: Vec<SourceFile>) -> PrivateSide {
    PrivateSide {
        full_name: "acme/backend".into(),
        files,
    }
}

fn public() -> PublicSide {
    PublicSide {
        full_name: "acme/frontend".into(),
        base_files: vec![],
        new_files: vec![],
        added: vec![],
        texts: vec![],
        unscannable: vec![],
    }
}

fn block(text: &str) -> AddedBlock {
    AddedBlock {
        path: "src/view.ts".into(),
        line: 10,
        text: text.into(),
    }
}

fn cats(f: &[Finding]) -> Vec<Category> {
    f.iter().map(|x| x.category).collect()
}

fn scan_block(priv_files: Vec<SourceFile>, text: &str) -> Vec<Finding> {
    let idx = Index::build(&[private(priv_files)]);
    let mut p = public();
    p.added.push(block(text));
    scan(&idx, &p)
}

fn overlap(f: &[Finding]) -> bool {
    f.iter().any(|x| x.category == Category::Overlap)
}

#[test]
fn verbatim_copy_is_flagged_with_public_excerpt_only() {
    let src = code("a", 20);
    let f = scan_block(vec![file("src/Ledger.java", &src)], &code("a", 20));
    let hit = f.iter().find(|x| x.category == Category::Overlap).unwrap();
    assert_eq!(
        hit.private_ref.as_deref(),
        Some("acme/backend:src/Ledger.java")
    );
    assert_eq!(
        hit.location,
        Location::File {
            path: "src/view.ts".into(),
            line: 10
        }
    );
    assert!(hit.excerpt.len() <= 160);
}

#[test]
fn reindented_and_resplit_copies_are_flagged() {
    let src = code("b", 20);
    let idx = vec![file("a.java", &src)];
    let reindented: String = src.lines().map(|l| format!("\t{}\n", l.trim())).collect();
    assert!(overlap(&scan_block(idx.clone(), &reindented)));
    let one_line = src.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(overlap(&scan_block(idx.clone(), &one_line)));
    let words: Vec<&str> = src.split_whitespace().collect();
    let rewrapped: String = words
        .chunks(3)
        .map(|c| format!("{}\n", c.join(" ")))
        .collect();
    assert!(overlap(&scan_block(idx, &rewrapped)));
}

#[test]
fn comment_wrapped_copy_is_flagged() {
    let src = code("c", 20);
    let idx = vec![file("a.java", &src)];
    assert!(overlap(&scan_block(idx.clone(), &format!("/*\n{src}*/"))));
    let per_line: String = src.lines().map(|l| format!("// {}\n", l.trim())).collect();
    assert!(overlap(&scan_block(idx.clone(), &per_line)));
    let hashes: String = src.lines().map(|l| format!("# {l}\n")).collect();
    assert!(overlap(&scan_block(idx, &hashes)));
}

#[test]
fn encoded_copies_are_flagged() {
    let src = code("d", 20);
    let idx = vec![file("a.java", &src)];
    let b64 = base64::engine::general_purpose::STANDARD.encode(&src);
    let wrapped: String = b64
        .as_bytes()
        .chunks(76)
        .map(|c| format!("{}\n", String::from_utf8_lossy(c)))
        .collect();
    assert!(overlap(&scan_block(
        idx.clone(),
        &format!("const blob = \"{b64}\";")
    )));
    assert!(overlap(&scan_block(idx.clone(), &wrapped)));
    let hex: String = src.bytes().map(|b| format!("{b:02x}")).collect();
    assert!(overlap(&scan_block(idx, &format!("0x{hex}"))));
}

#[test]
fn commit_message_carrying_code_is_flagged() {
    let src = code("e", 20);
    let idx = Index::build(&[private(vec![file("a.java", &src)])]);
    let mut p = public();
    p.texts
        .push(("commit 1a2b3c4 message".into(), format!("wip\n\n{src}")));
    let f = scan(&idx, &p);
    assert_eq!(f.len(), 1);
    assert_eq!(
        f[0].location,
        Location::Text {
            label: "commit 1a2b3c4 message".into()
        }
    );
}

#[test]
fn shared_imports_and_braces_do_not_overlap() {
    let boiler =
        "import java.util.List;\nimport java.util.Map;\npackage a.b.c;\n}\n}\n);\n".repeat(20);
    let f = scan_block(vec![file("a.java", &boiler)], &boiler);
    assert!(!overlap(&f), "{f:?}");
}

#[test]
fn single_kgram_idiom_does_not_overlap() {
    let idiom: Vec<String> = (0..20).map(|i| format!("w{i}")).collect();
    let line = format!("{}\n", idiom.join(" "));
    let f = scan_block(
        vec![file("a.java", &format!("{line}{}", code("f", 20)))],
        &line,
    );
    assert!(!overlap(&f), "{f:?}");
}

#[test]
fn unrelated_code_does_not_overlap() {
    let f = scan_block(vec![file("a.java", &code("g", 30))], &code("h", 30));
    assert!(!overlap(&f), "{f:?}");
}

#[test]
fn copy_already_in_the_public_base_is_not_flagged() {
    let src = code("i", 20);
    let idx = Index::build(&[private(vec![file("a.java", &src)])]);
    let mut p = public();
    p.base_files.push(file("src/old.ts", &src));
    p.added.push(block(&src));
    assert!(!overlap(&scan(&idx, &p)));
}

#[test]
fn foreign_files_are_flagged() {
    let idx = Index::build(&[private(vec![
        file("src/main/App.java", "class App {}"),
        file("docs/guide.md", "hi"),
        file("scripts/run.sh", "echo"),
        file("Dockerfile", "FROM x"),
    ])]);
    let mut p = public();
    p.base_files = vec![file("src/a.ts", "x"), file("scripts/other.sh", "y")];
    p.new_files = vec![
        "src/Thing.java".into(),
        "docs/guide.md".into(),
        "config/application-prod.yml".into(),
        "pom.xml".into(),
        "mvnw".into(),
        "x/db/changelog/001.xml".into(),
        "data/seed.sql".into(),
    ];
    let f = scan(&idx, &p);
    let paths: Vec<String> = f
        .iter()
        .map(|x| match &x.location {
            Location::File { path, .. } => path.clone(),
            _ => String::new(),
        })
        .collect();
    for want in [
        "src/Thing.java",
        "docs/guide.md",
        "config/application-prod.yml",
        "pom.xml",
        "mvnw",
        "x/db/changelog/001.xml",
        "data/seed.sql",
    ] {
        assert!(paths.contains(&want.to_string()), "{want} in {paths:?}");
    }
    assert!(cats(&f).iter().all(|c| *c == Category::ForeignFile));
}

#[test]
fn ordinary_new_files_are_not_foreign() {
    let idx = Index::build(&[private(vec![
        file("Dockerfile", "FROM x"),
        file("scripts/run.sh", "echo"),
        file("README.md", "hi"),
    ])]);
    let mut p = public();
    p.base_files = vec![
        file("src/a.ts", "x"),
        file("README.md", "y"),
        file("a.sh", "z"),
    ];
    p.new_files = vec![
        "Dockerfile".into(),
        "src/b.ts".into(),
        "scripts/other.sh".into(),
        "README.md".into(),
    ];
    assert!(scan(&idx, &p).is_empty());
}

#[test]
fn unscannable_entries_become_findings() {
    let idx = Index::build(&[]);
    let mut p = public();
    p.unscannable = vec![("a.bin".into(), "binary file".into())];
    let f = scan(&idx, &p);
    assert_eq!(cats(&f), vec![Category::Unscannable]);
    assert_eq!(f[0].excerpt, "binary file");
}

fn secret_findings(text: &str) -> Vec<Finding> {
    scan_block(vec![], text)
}

#[test]
fn known_secret_prefixes_are_flagged_and_redacted() {
    let gh = format!("{}{}", "ghp_", "a1B2".repeat(9));
    let aws = format!("{}{}", "AKIA", "ABCDEFGH23456789");
    let pem = format!("{} {} {}", "-----BEGIN", "RSA PRIVATE", "KEY-----");
    let slack = format!("{}{}", "xoxb-", "123456789012-abcdef");
    let jwt = format!("{}.{}.{}", "eyJhbGciOiJI", "eyJzdWIiOiIx", "c2lnbmF0dXJl");
    let oai = format!("{}{}", "sk-", "Ab1".repeat(14));
    for s in [&gh, &aws, &pem, &slack, &jwt, &oai] {
        let f = secret_findings(&format!("const k = '{s}';"));
        assert_eq!(cats(&f), vec![Category::Secret], "{s}");
        assert!(!f[0].excerpt.contains(s.as_str()) || s == &pem);
    }
}

#[test]
fn credential_literal_needs_entropy_and_no_placeholder() {
    let real = r#"const password = "Zq7!pL2x9Vw#kD4rT8nB";"#;
    assert_eq!(cats(&secret_findings(real)), vec![Category::Secret]);
    assert_eq!(
        cats(&secret_findings(&format!(
            "{{\"api_key\": \"{}{}\"}}",
            "Zq7pL2x9Vw", "4kD4rT8nBm5"
        ))),
        vec![Category::Secret]
    );
    for ok in [
        "const token = localStorage.getItem('token');",
        r#"const password = "aaaaaaaaaaaaaaaaaaaaaaaa";"#,
        r#"const password = "<your-password-here-xyz>";"#,
        r#"const secret = "${SECRET_FROM_ENV_VARIABLE}";"#,
        r#"const token = "changeme-changeme-changeme";"#,
        r#"const label = "Zq7!pL2x9Vw#kD4rT8nB";"#,
        r#"const password = "short";"#,
    ] {
        assert!(secret_findings(ok).is_empty(), "{ok}");
    }
}

fn names_index() -> Index {
    Index::build(&[private(vec![
        file(
            "src/main/java/com/acme/billing/Ledger.java",
            "package com.acme.billing.core;\nclass Ledger {}\n",
        ),
        file(
            "src/main/resources/application.yml",
            "url: jdbc:mysql://db01.acme-internal.io:3306/x\nhost: 198.18.30.40\nkey: ${ACME_LEDGER_SECRET}\nlocal: 127.0.0.1\nlh: localhost\nnode: ${NODE_ENV}\nlog: ${LOG_LEVEL:info}\nu: ${GITHUB_TOKEN}\ndoc: 192.0.2.7\n",
        ),
    ])])
}

fn name_hits(text: &str) -> Vec<Finding> {
    let idx = names_index();
    let mut p = public();
    p.added.push(block(text));
    scan(&idx, &p)
}

#[test]
fn internal_names_are_flagged() {
    for (text, kind) in [
        ("import com.acme.billing.core.Ledger", "package prefix"),
        ("fetch('https://db01.acme-internal.io/x')", "hostname"),
        ("const ip = '198.18.30.40';", "ip address"),
        ("process.env.ACME_LEDGER_SECRET", "env var"),
    ] {
        let f = name_hits(text);
        assert_eq!(cats(&f), vec![Category::InternalName], "{text}");
        assert_eq!(f[0].private_ref.as_deref(), Some(kind));
    }
}

#[test]
fn generic_names_are_not_flagged() {
    for t in [
        "http://localhost:8080 and 127.0.0.1 and 192.0.2.7",
        "process.env.NODE_ENV, LOG_LEVEL, GITHUB_TOKEN",
        "acme_ledger_secret lower case is another name",
        "198.18.30.400 and 1198.18.30.40",
    ] {
        assert!(name_hits(t).is_empty(), "{t}");
    }
}

#[test]
fn internal_name_present_in_the_public_base_is_dropped() {
    let idx = names_index();
    let mut p = public();
    p.base_files
        .push(file("README.md", "see ACME_LEDGER_SECRET"));
    p.added.push(block("use ACME_LEDGER_SECRET here"));
    assert!(scan(&idx, &p).is_empty());
}

#[test]
fn pull_request_title_with_an_internal_package_is_flagged() {
    let idx = names_index();
    let mut p = public();
    p.texts.push((
        "pull request title".into(),
        "Port com.acme.billing.core".into(),
    ));
    let f = scan(&idx, &p);
    assert_eq!(f.len(), 1);
    assert_eq!(
        f[0].location,
        Location::Text {
            label: "pull request title".into()
        }
    );
}

#[test]
fn findings_are_sorted_and_deduplicated_per_location() {
    let idx = names_index();
    let mut p = public();
    p.added
        .push(block("198.18.30.40 and db01.acme-internal.io"));
    p.added.push(AddedBlock {
        path: "a.ts".into(),
        line: 1,
        text: "198.18.30.40".into(),
    });
    let a = scan(&idx, &p);
    let b = scan(&idx, &p);
    assert_eq!(a, b);
    assert_eq!(a.len(), 2);
    assert!(matches!(&a[0].location, Location::File { path, .. } if path == "a.ts"));
}

fn git_env(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .current_dir(dir)
        .env_clear()
        .env("PATH", std::env::var_os("PATH").unwrap())
        .env("HOME", "/nonexistent")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .args(["-c", "user.name=Tester", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false", "-c", "core.autocrlf=false"])
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

fn write(dir: &Path, path: &str, bytes: &[u8]) {
    let p = dir.join(path);
    std::fs::create_dir_all(p.parent().unwrap()).unwrap();
    std::fs::write(p, bytes).unwrap();
}

fn commit(dir: &Path, msg: &str) -> String {
    git_env(dir, &["add", "-A"]);
    git_env(dir, &["commit", "-q", "-m", msg]);
    git_env(dir, &["rev-parse", "HEAD"])
}

fn repo() -> tempfile::TempDir {
    let d = tempfile::tempdir().unwrap();
    git_env(d.path(), &["init", "-q", "-b", "main"]);
    d
}

#[test]
fn load_private_dedupes_revs_and_skips_binary_and_large() {
    let d = repo();
    write(d.path(), "a.txt", b"one\n");
    write(d.path(), "bin.dat", b"ab\0cd");
    write(d.path(), "big.txt", &vec![b'x'; 600 * 1024]);
    let r1 = commit(d.path(), "one");
    write(d.path(), "b.txt", b"two\n");
    let r2 = commit(d.path(), "two");
    let side = load_private(d.path(), "acme/backend", &[r1, r2]).unwrap();
    let mut paths: Vec<&str> = side.files.iter().map(|f| f.path.as_str()).collect();
    paths.sort();
    assert_eq!(paths, vec!["a.txt", "b.txt"]);
    assert_eq!(side.full_name, "acme/backend");
}

#[test]
fn load_public_scans_every_commit_including_added_then_deleted() {
    let d = repo();
    write(d.path(), "keep.ts", b"let a = 1;\n");
    let base = commit(d.path(), "base");
    let secret = code("z", 3);
    write(d.path(), "src/leak.ts", secret.as_bytes());
    commit(d.path(), "add leak\n\nbody text");
    git_env(d.path(), &["rm", "-q", "src/leak.ts"]);
    write(d.path(), "keep.ts", b"let a = 1;\nlet b = 2;\nlet c = 3;\n");
    let head = commit(d.path(), "remove it");
    let side = load_public(
        d.path(),
        "acme/frontend",
        &base,
        &head,
        vec![("branch".into(), "feat/x".into())],
    )
    .unwrap();
    assert!(side.new_files.contains(&"src/leak.ts".to_string()));
    let leak = side.added.iter().find(|b| b.path == "src/leak.ts").unwrap();
    assert_eq!(leak.line, 1);
    assert_eq!(leak.text.trim_end(), secret.trim_end());
    let keep = side.added.iter().find(|b| b.path == "keep.ts").unwrap();
    assert_eq!(
        (keep.line, keep.text.as_str()),
        (2, "let b = 2;\nlet c = 3;")
    );
    let labels: Vec<&str> = side.texts.iter().map(|t| t.0.as_str()).collect();
    assert!(labels.iter().any(|l| l.ends_with("message")));
    assert!(labels.iter().any(|l| l.ends_with("author")));
    assert!(labels.iter().any(|l| l.ends_with("committer")));
    assert!(labels.contains(&"branch"));
    assert!(side.texts.iter().any(|t| t.1 == "Tester <t@example.com>"));
    assert_eq!(side.base_files.len(), 1);
}

#[test]
fn added_then_deleted_code_is_caught_by_overlap() {
    let d = repo();
    write(d.path(), "keep.ts", b"let a = 1;\n");
    let base = commit(d.path(), "base");
    let src = code("y", 20);
    write(d.path(), "src/leak.ts", src.as_bytes());
    commit(d.path(), "add");
    git_env(d.path(), &["rm", "-q", "src/leak.ts"]);
    let head = commit(d.path(), "remove");
    let side = load_public(d.path(), "acme/frontend", &base, &head, vec![]).unwrap();
    let idx = Index::build(&[private(vec![file("Ledger.java", &src)])]);
    assert!(overlap(&scan(&idx, &side)));
}

#[test]
fn load_public_reports_binary_gitlink_symlink_gitmodules_and_large() {
    let d = repo();
    write(d.path(), "keep.ts", b"x\n");
    let base = commit(d.path(), "base");
    write(d.path(), "img.png", b"\x89PNG\0\0data");
    write(d.path(), "huge.txt", &vec![b'q'; 600 * 1024]);
    write(
        d.path(),
        ".gitmodules",
        b"[submodule \"s\"]\n\tpath = s\n\turl = https://example.com/s.git\n",
    );
    git_env(d.path(), &["add", "-A"]);
    git_env(
        d.path(),
        &[
            "update-index",
            "--add",
            "--cacheinfo",
            "160000,1111111111111111111111111111111111111111,vendor/sub",
        ],
    );
    std::os::unix::fs::symlink("/etc/passwd", d.path().join("link")).unwrap();
    git_env(d.path(), &["add", "link"]);
    git_env(d.path(), &["commit", "-q", "-m", "stuff"]);
    let head = git_env(d.path(), &["rev-parse", "HEAD"]);
    let side = load_public(d.path(), "acme/frontend", &base, &head, vec![]).unwrap();
    let get = |p: &str| {
        side.unscannable
            .iter()
            .find(|u| u.0 == p)
            .map(|u| u.1.clone())
    };
    assert_eq!(get("img.png").as_deref(), Some("binary file"));
    assert_eq!(get("vendor/sub").as_deref(), Some("gitlink (submodule)"));
    assert_eq!(get("link").as_deref(), Some("symlink"));
    assert_eq!(get(".gitmodules").as_deref(), Some(".gitmodules changed"));
    assert_eq!(get("huge.txt").as_deref(), Some("larger than 512 KiB"));
    assert!(side
        .added
        .iter()
        .all(|b| b.path != "huge.txt" && b.path != "link"));
    let f = scan(&Index::build(&[]), &side);
    assert_eq!(
        f.iter()
            .filter(|x| x.category == Category::Unscannable)
            .count(),
        5
    );
}

#[test]
fn load_public_handles_paths_with_spaces_and_quotes() {
    let d = repo();
    write(d.path(), "keep.ts", b"x\n");
    let base = commit(d.path(), "base");
    write(d.path(), "dir with space/a b.ts", b"let q = 1;\n");
    write(d.path(), "q\"uote.ts", b"let r = 1;\n");
    let head = commit(d.path(), "more");
    let side = load_public(d.path(), "acme/frontend", &base, &head, vec![]).unwrap();
    let paths: Vec<&str> = side.added.iter().map(|b| b.path.as_str()).collect();
    assert!(paths.contains(&"dir with space/a b.ts"), "{paths:?}");
    assert!(paths.contains(&"q\"uote.ts"), "{paths:?}");
}

#[test]
fn lockfile_content_does_not_overlap() {
    let lock = code("lk", 30);
    let f = scan_block(vec![file("web/package-lock.json", &lock)], &lock);
    assert!(!overlap(&f), "{f:?}");
}

#[test]
fn commit_text_cannot_forge_records_or_inject_git_arguments() {
    let d = repo();
    write(d.path(), "keep.ts", b"let a = 1;\n");
    let base = commit(d.path(), "base");
    let out = d.path().join("..").join(format!(
        "forged-{}.txt",
        d.path().file_name().unwrap().to_string_lossy()
    ));
    let msg = format!(
        "hello\x1e{base}\x1f--output={}\x1fX <x@example.com>\x1fY <y@example.com>\x1fforged",
        out.display()
    );
    write(d.path(), "b.ts", b"let b = 2;\n");
    let head = commit(d.path(), &msg);
    let side = load_public(d.path(), "acme/frontend", &base, &head, vec![]).unwrap();
    assert!(!out.exists());
    let messages: Vec<&(String, String)> = side
        .texts
        .iter()
        .filter(|t| t.0.ends_with("message"))
        .collect();
    assert_eq!(messages.len(), 1);
    assert!(messages[0].1.contains("--output="));
    assert!(messages[0].1.contains('\x1e') && messages[0].1.contains('\x1f'));
    assert!(messages[0].1.ends_with("forged"));
}

#[test]
fn code_copied_with_junk_lines_that_a_later_commit_removes_is_caught() {
    let d = repo();
    write(d.path(), "keep.ts", b"let a = 1;\n");
    let base = commit(d.path(), "base");
    let src = code("n", 20);
    let junked: String = src
        .lines()
        .enumerate()
        .map(|(i, l)| format!("{l}\nlet junk{i} = {i};\n"))
        .collect();
    write(d.path(), "src/copy.ts", junked.as_bytes());
    commit(d.path(), "add");
    write(d.path(), "src/copy.ts", format!("{src}\n").as_bytes());
    let head = commit(d.path(), "tidy");
    let side = load_public(d.path(), "acme/frontend", &base, &head, vec![]).unwrap();
    let idx = Index::build(&[private(vec![file("Ledger.java", &src)])]);
    assert!(overlap(&scan(&idx, &side)));
}

#[test]
fn a_modified_existing_lockfile_is_exempt_from_the_size_limit() {
    let d = repo();
    write(d.path(), "web/package-lock.json", &vec![b'a'; 600 * 1024]);
    let base = commit(d.path(), "base");
    write(d.path(), "web/package-lock.json", &vec![b'b'; 700 * 1024]);
    let head = commit(d.path(), "bump");
    let side = load_public(d.path(), "acme/frontend", &base, &head, vec![]).unwrap();
    assert!(side.unscannable.is_empty(), "{:?}", side.unscannable);
}

#[test]
fn a_new_or_misplaced_oversized_lockfile_is_unscannable() {
    let d = repo();
    write(d.path(), "web/package-lock.json", b"{}\n");
    let base = commit(d.path(), "base");
    write(d.path(), "package-lock.json", &vec![b'c'; 700 * 1024]);
    write(d.path(), "other/yarn.lock", &vec![b'd'; 700 * 1024]);
    let head = commit(d.path(), "add");
    let side = load_public(d.path(), "acme/frontend", &base, &head, vec![]).unwrap();
    let paths: Vec<&str> = side.unscannable.iter().map(|u| u.0.as_str()).collect();
    assert_eq!(paths, vec!["other/yarn.lock", "package-lock.json"]);
}

#[test]
fn zero_width_characters_do_not_hide_a_copy() {
    let src = code("zw", 20);
    let idx = vec![file("a.java", &src)];
    let hidden = src
        .replace("ledger", "led\u{200b}ger")
        .replace("total", "to\u{200d}tal")
        .replace("entry", "en\u{feff}try")
        .replace("rate", "ra\u{2060}te")
        .replace("apply", "ap\u{ad}ply");
    assert!(overlap(&scan_block(idx, &hidden)));
}

#[test]
fn require_inside_a_long_line_is_not_trivial() {
    let long: String = (0..8)
        .map(|i| {
            format!(
                "register{i}(require(\"./ledger{i}\"), ledger.apply(entry{i}, rate{i} * 3), ledger.apply(other{i}, level{i} * 4), ledger.apply(third{i}, step{i} * 5), total{i});\n"
            )
        })
        .collect();
    assert!(overlap(&scan_block(vec![file("a.js", &long)], &long)));
    let plain: String = (0..30)
        .map(|i| format!("const mod{i} = require('./module{i}/index');\n"))
        .collect();
    assert!(!overlap(&scan_block(vec![file("a.js", &plain)], &plain)));
}

#[test]
fn names_next_to_non_ascii_letters_are_still_flagged() {
    let f = name_hits("const x = \"\u{e9}ACME_LEDGER_SECRET\u{e9}\";");
    assert_eq!(cats(&f), vec![Category::InternalName]);
}

fn b64(s: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(s)
}

#[test]
fn encoded_secrets_are_flagged_at_the_original_location() {
    let gh = format!("{}{}", "ghp_", "a1B2".repeat(9));
    let aws = format!("{}{}", "AKIA", "ABCDEFGH23456789");
    let esc: String = aws.bytes().map(|b| format!("\\x{b:02x}")).collect();
    let ox: String = aws.bytes().map(|b| format!("0x{b:02x}, ")).collect();
    let spaced: String = aws
        .bytes()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(" ");
    let uni: String = aws
        .chars()
        .map(|c| format!("\\u{:04x}", c as u32))
        .collect();
    let cases = [
        format!("const k = \"{}\";", b64(&format!("token {gh} end"))),
        format!("const k = \"{esc}\";"),
        format!("const k = [{ox}];"),
        format!("// bytes: {spaced}"),
        format!("const k = \"{uni}\";"),
    ];
    for text in cases {
        let f = secret_findings(&format!("let first = 1;\n{text}"));
        assert_eq!(cats(&f), vec![Category::Secret], "{text}");
        assert_eq!(
            f[0].location,
            Location::File {
                path: "src/view.ts".into(),
                line: 11
            },
            "{text}"
        );
        assert!(!f[0].excerpt.contains(&aws), "{text}");
    }
}

#[test]
fn encoded_internal_names_are_flagged() {
    let name = "db01.acme-internal.io";
    let esc: String = name.bytes().map(|b| format!("\\x{b:02x}")).collect();
    let uni: String = name
        .chars()
        .map(|c| format!("\\u{:04x}", c as u32))
        .collect();
    for text in [
        format!("const h = \"{}\";", b64(&format!("https://{name}/path"))),
        format!("const h = \"{esc}\";"),
        format!("const h = \"{uni}\";"),
    ] {
        let f = name_hits(&text);
        assert_eq!(cats(&f), vec![Category::InternalName], "{text}");
    }
}

#[test]
fn changed_public_paths_are_scanned_as_text() {
    let d = repo();
    write(d.path(), "keep.ts", b"x\n");
    let base = commit(d.path(), "base");
    let name = format!("keys/{}{}.txt", "AKIA", "ABCDEFGH23456789");
    write(d.path(), &name, b"harmless\n");
    let head = commit(d.path(), "add");
    let side = load_public(d.path(), "acme/frontend", &base, &head, vec![]).unwrap();
    assert!(side.texts.iter().any(|t| t.0 == "path" && t.1 == name));
    let f = scan(&Index::build(&[]), &side);
    assert_eq!(cats(&f), vec![Category::Secret]);
    assert_eq!(
        f[0].location,
        Location::Text {
            label: "path".into()
        }
    );
}
