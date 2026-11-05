use super::state::Row;

pub(super) fn rank(rows: &[Row], query: &str, hits: Option<&[String]>) -> Vec<usize> {
    let needle = query.trim().to_lowercase();
    let names: Vec<Option<String>> = rows
        .iter()
        .map(|r| r.name.as_ref().map(|n| n.to_lowercase()))
        .collect();
    let tier = |i: usize| {
        names[i].as_deref().and_then(|n| {
            if n == needle {
                Some(0)
            } else if n.starts_with(&needle) {
                Some(1)
            } else if n.contains(&needle) {
                Some(2)
            } else {
                None
            }
        })
    };
    let mut out: Vec<usize> = Vec::new();
    if !needle.is_empty() {
        for t in 0..3 {
            out.extend((0..rows.len()).filter(|&i| tier(i) == Some(t)));
        }
    }
    let rest: Vec<usize> = match hits {
        Some(ids) => ids
            .iter()
            .filter_map(|id| rows.iter().position(|r| &r.id == id))
            .collect(),
        None => (0..rows.len())
            .filter(|&i| {
                let r = &rows[i];
                r.project.to_lowercase().contains(&needle)
                    || r.text.to_lowercase().contains(&needle)
            })
            .collect(),
    };
    let mut seen = vec![false; rows.len()];
    for &i in &out {
        seen[i] = true;
    }
    for i in rest {
        if !seen[i] {
            seen[i] = true;
            out.push(i);
        }
    }
    out
}
