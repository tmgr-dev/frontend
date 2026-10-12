const SCISSORS: &str = "# ------------------------ >8 ------------------------";

fn is_comment(line: &str) -> bool {
    line.starts_with('#')
}

fn is_trailer_line(line: &str) -> bool {
    if line.starts_with([' ', '\t']) {
        return true;
    }
    match line.split_once(':') {
        Some((token, _)) => {
            !token.is_empty()
                && token
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
        }
        None => false,
    }
}

pub fn has_refs(message: &str) -> bool {
    message
        .lines()
        .take_while(|l| !l.starts_with(SCISSORS))
        .filter(|l| !is_comment(l))
        .any(|l| {
            l.split_once(':')
                .is_some_and(|(t, _)| t.eq_ignore_ascii_case("refs"))
        })
}

pub fn add_refs(message: &str, key: &str) -> Option<String> {
    if has_refs(message) {
        return None;
    }
    let lines: Vec<&str> = message.split_inclusive('\n').collect();
    let tail_start = lines
        .iter()
        .position(|l| l.trim_end_matches(['\n', '\r']) == SCISSORS)
        .unwrap_or(lines.len());
    let content_end = lines[..tail_start]
        .iter()
        .rposition(|l| !is_comment(l) && !l.trim().is_empty())?
        + 1;
    let first_content = lines[..content_end]
        .iter()
        .position(|l| !is_comment(l) && !l.trim().is_empty())?;
    let block_start = lines[..content_end]
        .iter()
        .rposition(|l| l.trim().is_empty())
        .map_or(0, |i| i + 1);
    let block: Vec<&str> = lines[block_start.max(first_content)..content_end]
        .iter()
        .filter(|l| !is_comment(l))
        .map(|l| l.trim_end_matches(['\n', '\r']))
        .collect();
    let is_trailer_block = block_start > first_content
        && !block.is_empty()
        && block.iter().all(|l| is_trailer_line(l))
        && !block[0].starts_with([' ', '\t']);

    let mut out = String::new();
    for l in &lines[..content_end] {
        out.push_str(l);
    }
    if !out.ends_with('\n') {
        out.push('\n');
    }
    if !is_trailer_block {
        out.push('\n');
    }
    out.push_str("Refs: ");
    out.push_str(key);
    out.push('\n');
    for l in &lines[content_end..] {
        out.push_str(l);
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn appends_after_a_blank_line() {
        assert_eq!(
            add_refs("fix: thing\n", "TM-1").unwrap(),
            "fix: thing\n\nRefs: TM-1\n"
        );
        assert_eq!(
            add_refs("fix: thing", "TM-1").unwrap(),
            "fix: thing\n\nRefs: TM-1\n"
        );
        assert_eq!(
            add_refs("subject\n\nbody text\nmore\n", "TM-1").unwrap(),
            "subject\n\nbody text\nmore\n\nRefs: TM-1\n"
        );
    }

    #[test]
    fn joins_an_existing_trailer_block() {
        assert_eq!(
            add_refs(
                "subject\n\nbody\n\nSigned-off-by: A <a@example.com>\n",
                "TM-1"
            )
            .unwrap(),
            "subject\n\nbody\n\nSigned-off-by: A <a@example.com>\nRefs: TM-1\n"
        );
    }

    #[test]
    fn a_subject_that_looks_like_a_trailer_is_not_a_trailer_block() {
        assert_eq!(
            add_refs("fix: thing\n", "TM-1").unwrap(),
            "fix: thing\n\nRefs: TM-1\n"
        );
    }

    #[test]
    fn leaves_comment_lines_after_the_trailer() {
        let msg = "subject\n\n# Please enter the commit message\n# more\n";
        assert_eq!(
            add_refs(msg, "TM-1").unwrap(),
            "subject\n\nRefs: TM-1\n\n# Please enter the commit message\n# more\n"
        );
    }

    #[test]
    fn comment_lines_are_not_content() {
        assert_eq!(add_refs("# only a comment\n", "TM-1"), None);
        assert_eq!(add_refs("", "TM-1"), None);
        assert_eq!(add_refs("\n\n  \n", "TM-1"), None);
    }

    #[test]
    fn existing_refs_wins_case_insensitively_but_comments_do_not_count() {
        assert_eq!(add_refs("s\n\nRefs: TM-9\n", "TM-1"), None);
        assert_eq!(add_refs("s\n\nrefs: TM-9\n", "TM-1"), None);
        assert!(add_refs("s\n\n# Refs: TM-9\n", "TM-1").is_some());
    }

    #[test]
    fn verbose_commits_keep_the_diff_untouched() {
        let msg = format!("subject\n\n# comment\n{SCISSORS}\ndiff --git a/x b/x\nRefs: not-ours\n");
        let out = add_refs(&msg, "TM-1").unwrap();
        assert!(out.starts_with("subject\n\nRefs: TM-1\n"));
        assert!(out.ends_with("diff --git a/x b/x\nRefs: not-ours\n"));
        assert!(!has_refs(&msg));
    }

    #[test]
    fn crlf_messages_stay_valid() {
        let out = add_refs("subject\r\n", "TM-1").unwrap();
        assert!(out.contains("Refs: TM-1\n"));
        assert!(out.starts_with("subject\r\n"));
    }
}
