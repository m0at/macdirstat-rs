/// Fuzzy match: all needle chars must appear in haystack in order (case-insensitive).
/// Returns Some(score) on match, None on no match. Higher = better.
pub fn score(haystack: &str, needle: &str) -> Option<i32> {
    if needle.is_empty() { return Some(0); }

    let h: Vec<char> = haystack.to_lowercase().chars().collect();
    let n: Vec<char> = needle.to_lowercase().chars().collect();

    // All needle chars must appear in order — if not, no match
    let mut hi = 0usize;
    for &nc in &n {
        match h[hi..].iter().position(|&c| c == nc) {
            Some(off) => hi += off + 1,
            None => return None,
        }
    }

    // Score: prefer exact > starts_with > contains substring > scattered fuzzy
    let hl = haystack.to_lowercase();
    let nl = needle.to_lowercase();
    let mut s: i32 = 0;

    if hl == nl              { s += 2000; }
    else if hl.starts_with(&nl) { s += 1200; }
    else if hl.contains(&nl)    { s += 600;  }
    else {
        // Score consecutive run bonus
        let mut run = 0i32;
        let mut prev: Option<usize> = None;
        let mut pos = 0usize;
        for &nc in &n {
            let found = h[pos..].iter().position(|&c| c == nc).unwrap(); // already verified above
            let abs = pos + found;
            if prev == Some(abs - 1) { run += 1; s += run * 40; } else { run = 0; s += 10; }
            prev = Some(abs);
            pos = abs + 1;
        }
    }

    // Penalty for long haystack (shorter = more specific match)
    s -= (haystack.len() as i32).min(50);
    Some(s)
}

/// Match and return score against the basename of a path string.
pub fn score_path(path_str: &str, needle: &str) -> Option<i32> {
    let basename = path_str.rsplit('/').next().unwrap_or(path_str);
    // Score basename (primary) and full path (secondary bonus)
    let base_score = score(basename, needle)?;
    let full_bonus = score(path_str, needle).unwrap_or(0) / 4;
    Some(base_score + full_bonus)
}

/// Filter and sort a list of items by fuzzy score descending.
/// `key_fn` extracts the string to match against.
pub fn filter_sorted<'a, T, F>(items: &'a [T], needle: &str, key_fn: F) -> Vec<(i32, &'a T)>
where
    F: Fn(&T) -> &str,
{
    if needle.is_empty() { return items.iter().map(|t| (0, t)).collect(); }
    let mut out: Vec<(i32, &T)> = items.iter()
        .filter_map(|t| score(key_fn(t), needle).map(|s| (s, t)))
        .collect();
    out.sort_unstable_by(|a, b| b.0.cmp(&a.0));
    out
}
