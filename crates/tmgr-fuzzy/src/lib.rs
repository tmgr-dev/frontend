pub fn score(query: &str, text: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }
    let hay: Vec<char> = text.to_lowercase().chars().collect();
    let mut at = 0;
    let mut total = 0;
    let mut last: Option<usize> = None;
    for q in query.to_lowercase().chars() {
        let found = hay[at..].iter().position(|c| *c == q)? + at;
        total += 10;
        if last.is_some_and(|l| l + 1 == found) {
            total += 15;
        }
        if found == 0 || !hay[found - 1].is_alphanumeric() {
            total += 8;
        }
        total -= (found - last.map_or(0, |l| l + 1)).min(10) as i32;
        last = Some(found);
        at = found + 1;
    }
    Some(total - (hay.len() as i32) / 8)
}

pub fn matched(query: &str, text: &str) -> Vec<usize> {
    let hay: Vec<char> = text.to_lowercase().chars().collect();
    let mut at = 0;
    let mut out = Vec::new();
    for q in query.to_lowercase().chars() {
        match hay[at..].iter().position(|c| *c == q) {
            Some(p) => {
                out.push(at + p);
                at += p + 1;
            }
            None => return Vec::new(),
        }
    }
    out
}

pub fn rank<T>(query: &str, items: Vec<T>, text: impl Fn(&T) -> String) -> Vec<T> {
    let mut scored: Vec<(i32, usize, T)> = items
        .into_iter()
        .enumerate()
        .filter_map(|(i, it)| score(query, &text(&it)).map(|s| (s, i, it)))
        .collect();
    scored.sort_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(&b.1)));
    scored.into_iter().map(|(_, _, it)| it).collect()
}
