//! Shared anchored glob syntax for live and indexed search.
//! `*` and `?` are wildcards; spaces, tabs and semicolons separate alternatives.
pub(super) fn pattern(pattern: &str) -> String {
    let parts: Vec<String> = pattern
        .split([';', ' ', '\t'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|g| {
            let mut out = String::new();
            for ch in g.chars() {
                match ch {
                    '*' => out.push_str(".*"),
                    '?' => out.push('.'),
                    c if ".+^${}()|[]\\".contains(c) => {
                        out.push('\\');
                        out.push(c);
                    }
                    c => out.push(c),
                }
            }
            out
        })
        .collect();
    let body = if parts.is_empty() {
        ".*".to_string()
    } else {
        parts.join("|")
    };
    format!("^(?:{})$", body)
}
