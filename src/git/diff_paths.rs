//! Parse file paths from `git diff` output.
//!
//! Only the first file section's headers are inspected. Hunk bodies (from the
//! first `@@` line on) are never read so hunk content that looks like
//! metadata cannot be mistaken for headers.

/// Old (`a/` side) and new (`b/` side) paths for the first file in a diff.
///
/// `None` means that side is `/dev/null`:
/// - new file → `old` is `None`
/// - deleted file → `new` is `None`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffFilePaths {
    pub old: Option<String>,
    pub new: Option<String>,
}

/// Extract the first file section's paths from `git diff` output.
///
/// Looks at the first `diff --git a/… b/…` header (including C-style quoted
/// paths and unquoted paths with spaces), then applies `rename from/to`,
/// `copy from/to`, `---`/`+++` (`/dev/null` → `None`) and
/// `new file mode` (`old` → `None`) / `deleted file mode` (`new` → `None`)
/// overrides. Stops at the first `@@` hunk header and at the next
/// `diff --git` (second file). Returns `None` when no headers are found.
pub(crate) fn paths_from_diff(diff: &str) -> Option<DiffFilePaths> {
    let mut old: Option<String> = None;
    let mut new: Option<String> = None;
    let mut seen_diff_git = false;
    let mut seen_any = false;

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            if seen_diff_git {
                break;
            }
            if let Some((o, n)) = parse_diff_git_line(line) {
                old = Some(o);
                new = Some(n);
            }
            seen_diff_git = true;
            seen_any = true;
            continue;
        }

        if line.starts_with("@@") {
            break;
        }

        if let Some(rest) = line.strip_prefix("rename from ") {
            if let Some(p) = decode_single_path(rest) {
                old = Some(p);
                seen_any = true;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("rename to ") {
            if let Some(p) = decode_single_path(rest) {
                new = Some(p);
                seen_any = true;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("copy from ") {
            if let Some(p) = decode_single_path(rest) {
                old = Some(p);
                seen_any = true;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("copy to ") {
            if let Some(p) = decode_single_path(rest) {
                new = Some(p);
                seen_any = true;
            }
            continue;
        }

        if is_mode_line(line, "new file mode") {
            old = None;
            seen_any = true;
            continue;
        }
        if is_mode_line(line, "deleted file mode") {
            new = None;
            seen_any = true;
            continue;
        }

        if let Some(rest) = line.strip_prefix("--- ") {
            if let Some(opt) = parse_ext_path(rest, "a/") {
                old = opt;
                seen_any = true;
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("+++ ") {
            if let Some(opt) = parse_ext_path(rest, "b/") {
                new = opt;
                seen_any = true;
            }
            continue;
        }

        if line.starts_with("Binary files ") || line.starts_with("Binary file ") {
            // Binary marker carries no paths beyond `/dev/null` handling.
            // Synthetic binary diffs still provide `diff --git` + file modes,
            // so this only needs to cover `/dev/null` sides.
            if line.starts_with("Binary files /dev/null and ")
                || line.starts_with("Binary file /dev/null and ")
            {
                old = None;
                seen_any = true;
            } else if line.contains(" and /dev/null differ") {
                new = None;
                seen_any = true;
            }
            continue;
        }
    }

    if !seen_any {
        return None;
    }
    Some(DiffFilePaths { old, new })
}

/// Return the new (`b/` side) path for a diff header.
///
/// Accepts either a single `diff --git …` line or a full (single-file) diff.
/// Falls back to the old path for deleted files so callers always get a
/// filename when one is known.
pub(crate) fn new_path_from_header(header: &str) -> Option<String> {
    let paths = paths_from_diff(header)?;
    paths.new.or(paths.old)
}

fn is_mode_line(line: &str, kind: &str) -> bool {
    if let Some(rest) = line.strip_prefix(kind) {
        rest.is_empty() || rest.starts_with(' ') || rest.starts_with('\t')
    } else {
        false
    }
}

fn strip_prefix_or_raw(path: &str, prefix: &str) -> String {
    if let Some(rest) = path.strip_prefix(prefix) {
        rest.to_string()
    } else {
        path.to_string()
    }
}

/// Decode a lone path token (for `rename from/to`, `copy from/to`).
/// C-quoted (`"…"`) tokens are C-unescaped; anything else is kept byte for
/// byte (no trimming) so trailing spaces survive.
fn decode_single_path(rest: &str) -> Option<String> {
    if rest.is_empty() {
        return None;
    }
    if rest.starts_with('"') {
        let (decoded, _) = parse_c_quoted(rest)?;
        Some(decoded)
    } else {
        Some(rest.to_string())
    }
}

/// Parse an extended-header path (`---`/`+++` remainder after the marker).
/// Returns `None` for malformed input, `Some(None)` for `/dev/null`,
/// `Some(Some(path))` otherwise. `prefix` is `a/` for `---`, `b/` for `+++`.
fn parse_ext_path(rest: &str, prefix: &str) -> Option<Option<String>> {
    if rest.is_empty() {
        return None;
    }
    if rest.starts_with('"') {
        let (decoded, _) = parse_c_quoted(rest)?;
        if decoded == "/dev/null" {
            return Some(None);
        }
        return Some(Some(strip_prefix_or_raw(&decoded, prefix)));
    }
    let path_part = rest.split('\t').next().unwrap_or(rest);
    if path_part == "/dev/null" {
        return Some(None);
    }
    Some(Some(strip_prefix_or_raw(path_part, prefix)))
}

fn parse_diff_git_line(line: &str) -> Option<(String, String)> {
    let rest = line.strip_prefix("diff --git ")?;
    if rest.is_empty() {
        return None;
    }
    if rest.starts_with('"') {
        let (first_raw, after_first) = parse_c_quoted(rest)?;
        let after = after_first.strip_prefix(' ')?;
        if after.is_empty() {
            return None;
        }
        if after.starts_with('"') {
            let (second_raw, _) = parse_c_quoted(after)?;
            let old = strip_prefix_or_raw(&first_raw, "a/");
            let new = strip_prefix_or_raw(&second_raw, "b/");
            Some((old, new))
        } else {
            let old = strip_prefix_or_raw(&first_raw, "a/");
            let new = strip_prefix_or_raw(after, "b/");
            Some((old, new))
        }
    } else {
        let mut candidates = Vec::new();
        for (idx, _) in rest.match_indices(" b/") {
            candidates.push(idx);
        }
        if candidates.is_empty() {
            return None;
        }
        if candidates.len() == 1 {
            let idx = candidates[0];
            let left = &rest[..idx];
            let right = &rest[idx + 3..];
            let old = strip_prefix_or_raw(left, "a/");
            return Some((old, right.to_string()));
        }
        for idx in &candidates {
            let left = &rest[..*idx];
            let right = &rest[idx + 3..];
            let old_cand = strip_prefix_or_raw(left, "a/");
            if old_cand == right {
                return Some((old_cand, right.to_string()));
            }
        }
        let idx = candidates[0];
        let left = &rest[..idx];
        let right = &rest[idx + 3..];
        let old = strip_prefix_or_raw(left, "a/");
        Some((old, right.to_string()))
    }
}

/// Parse a C-style quoted string starting at the opening `"`.
/// Returns the decoded string plus the remainder after the closing quote.
fn parse_c_quoted(input: &str) -> Option<(String, &str)> {
    if !input.starts_with('"') {
        return None;
    }
    let bytes = input.as_bytes();
    let mut out: Vec<u8> = Vec::new();
    let mut i = 1;
    while i < bytes.len() {
        let b = bytes[i];
        if b == b'"' {
            let remainder = &input[i + 1..];
            let s = String::from_utf8_lossy(&out).into_owned();
            return Some((s, remainder));
        } else if b == b'\\' {
            i += 1;
            if i >= bytes.len() {
                return None;
            }
            let e = bytes[i];
            match e {
                b'n' => out.push(b'\n'),
                b't' => out.push(b'\t'),
                b'r' => out.push(b'\r'),
                b'a' => out.push(0x07),
                b'b' => out.push(0x08),
                b'f' => out.push(0x0C),
                b'v' => out.push(0x0B),
                b'\\' => out.push(b'\\'),
                b'"' => out.push(b'"'),
                b'0'..=b'7' => {
                    let mut val: u32 = (e - b'0') as u32;
                    let mut count = 1;
                    while count < 3
                        && i + 1 < bytes.len()
                        && bytes[i + 1] >= b'0'
                        && bytes[i + 1] <= b'7'
                    {
                        i += 1;
                        val = val * 8 + (bytes[i] - b'0') as u32;
                        count += 1;
                    }
                    out.push((val & 0xFF) as u8);
                }
                _ => out.push(e),
            }
        } else {
            out.push(b);
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn header_diff(body: &str) -> String {
        body.to_string()
    }

    #[test]
    fn unquoted_simple() {
        let diff = header_diff(
            "diff --git a/foo.txt b/foo.txt\nindex abc..def 100644\n--- a/foo.txt\n+++ b/foo.txt\n@@ -1 +1 @@\n-old\n+new\n",
        );
        let p = paths_from_diff(&diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("foo.txt"));
        assert_eq!(p.new.as_deref(), Some("foo.txt"));
    }

    #[test]
    fn unquoted_with_spaces() {
        let diff = "diff --git a/foo bar/baz qux.txt b/foo bar/baz qux.txt\n--- a/foo bar/baz qux.txt\n+++ b/foo bar/baz qux.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("foo bar/baz qux.txt"));
        assert_eq!(p.new.as_deref(), Some("foo bar/baz qux.txt"));
        assert_eq!(
            new_path_from_header("diff --git a/foo bar/baz qux.txt b/foo bar/baz qux.txt"),
            Some("foo bar/baz qux.txt".to_string())
        );
    }

    #[test]
    fn filename_containing_b_slash_uses_equality() {
        // Raw path is `x b/y`; the separator is ambiguous without equality.
        let diff =
            "diff --git a/x b/y b/x b/y\n--- a/x b/y\n+++ b/x b/y\n@@ -1 +1 @@\n-old\n+new\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("x b/y"));
        assert_eq!(p.new.as_deref(), Some("x b/y"));
        assert_eq!(
            new_path_from_header("diff --git a/x b/y b/x b/y"),
            Some("x b/y".to_string())
        );
    }

    #[test]
    fn escaped_quote_and_backslash() {
        // File `foo"bar\baz`
        let diff = "diff --git \"a/foo\\\"bar\\\\baz\" \"b/foo\\\"bar\\\\baz\"\n--- \"a/foo\\\"bar\\\\baz\"\n+++ \"b/foo\\\"bar\\\\baz\"\n@@ -1 +1 @@\n-old\n+new\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("foo\"bar\\baz"));
        assert_eq!(p.new.as_deref(), Some("foo\"bar\\baz"));
    }

    #[test]
    fn escaped_newline_and_tab() {
        // File `foo\nbar\tbaz` (real newline/tab via escapes)
        let diff = "diff --git \"a/foo\\nbar\\tbaz\" \"b/foo\\nbar\\tbaz\"\n--- \"a/foo\\nbar\\tbaz\"\n+++ \"b/foo\\nbar\\tbaz\"\n@@ -1 +1 @@\n-old\n+new\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("foo\nbar\tbaz"));
        assert_eq!(p.new.as_deref(), Some("foo\nbar\tbaz"));
    }

    #[test]
    fn utf8_octal_decodes() {
        // `caf\u{e9}` with é as octal bytes \303\251
        let diff = "diff --git \"a/caf\\303\\251.txt\" \"b/caf\\303\\251.txt\"\n--- \"a/caf\\303\\251.txt\"\n+++ \"b/caf\\303\\251.txt\"\n@@ -1 +1 @@\n-old\n+new\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("café.txt"));
        assert_eq!(p.new.as_deref(), Some("café.txt"));
    }

    #[test]
    fn added_file() {
        let diff = "diff --git a/new.txt b/new.txt\nnew file mode 100644\nindex 0000000..abc\n--- /dev/null\n+++ b/new.txt\n@@ -0,0 +1 @@\n+hello\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old, None);
        assert_eq!(p.new.as_deref(), Some("new.txt"));
    }

    #[test]
    fn deleted_file() {
        let diff = "diff --git a/old.txt b/old.txt\ndeleted file mode 100644\nindex abc..0000000\n--- a/old.txt\n+++ /dev/null\n@@ -1 +0,0 @@\n-hello\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("old.txt"));
        assert_eq!(p.new, None);
        // new_path falls back to old so deleted files still have a name.
        assert_eq!(new_path_from_header(diff), Some("old.txt".to_string()));
    }

    #[test]
    fn new_file_mode_without_dashes_binary_style() {
        let diff = "diff --git a/img.png b/img.png\nnew file mode 100644\nindex 0000000..abc\nBinary files /dev/null and b/img.png differ\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old, None);
        assert_eq!(p.new.as_deref(), Some("img.png"));
    }

    #[test]
    fn renamed_paths_override() {
        let diff = "diff --git a/old name.txt b/new name.txt\nsimilarity index 90%\nrename from old name.txt\nrename to new name.txt\n--- a/old name.txt\n+++ b/new name.txt\n@@ -1 +1 @@\n-old\n+new\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("old name.txt"));
        assert_eq!(p.new.as_deref(), Some("new name.txt"));
        assert_eq!(new_path_from_header(diff), Some("new name.txt".to_string()));
    }

    #[test]
    fn renamed_quoted_override() {
        let diff = "diff --git \"a/foo\\\"old\" \"b/foo\\\"new\"\nsimilarity index 90%\nrename from \"foo\\\"old\"\nrename to \"foo\\\"new\"\n--- \"a/foo\\\"old\"\n+++ \"b/foo\\\"new\"\n@@ -1 +1 @@\n-old\n+new\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("foo\"old"));
        assert_eq!(p.new.as_deref(), Some("foo\"new"));
    }

    #[test]
    fn rename_only_without_dashes() {
        let diff = "diff --git a/old.txt b/new.txt\nsimilarity index 100%\nrename from old.txt\nrename to new.txt\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("old.txt"));
        assert_eq!(p.new.as_deref(), Some("new.txt"));
    }

    #[test]
    fn copy_from_to_override() {
        let diff = "diff --git a/orig.txt b/copy.txt\nsimilarity index 95%\ncopy from orig.txt\ncopy to copy.txt\n--- a/orig.txt\n+++ b/copy.txt\n@@ -1 +1 @@\n-x\n+y\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("orig.txt"));
        assert_eq!(p.new.as_deref(), Some("copy.txt"));
    }

    #[test]
    fn hunk_lines_pretending_metadata_are_ignored() {
        let diff = "diff --git a/real.txt b/real.txt\n--- a/real.txt\n+++ b/real.txt\n@@ -1,3 +1,3 @@\n context\ndiff --git a/fake b/fake\n--- a/fake\n+++ b/fake\n-rename from fake\n-rename to fake\n-new file mode 100644\n-deleted file mode 100644\n+newcontent\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("real.txt"));
        assert_eq!(p.new.as_deref(), Some("real.txt"));
    }

    #[test]
    fn hunk_minus_plus_pretending_headers_ignored() {
        let diff = "diff --git a/a.txt b/a.txt\n--- a/a.txt\n+++ b/a.txt\n@@ -1 +1 @@\n---- a/fake\n-+++ b/fake\n old\n+new\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("a.txt"));
        assert_eq!(p.new.as_deref(), Some("a.txt"));
    }

    #[test]
    fn no_headers_returns_none() {
        assert_eq!(paths_from_diff(""), None);
        assert_eq!(paths_from_diff("just some text\nno diff here\n"), None);
        assert_eq!(new_path_from_header("nothing"), None);
    }

    #[test]
    fn first_section_wins_for_multi_file() {
        let diff = "diff --git a/first.txt b/first.txt\n--- a/first.txt\n+++ b/first.txt\n@@ -1 +1 @@\n-a\n+b\ndiff --git a/second.txt b/second.txt\n--- a/second.txt\n+++ b/second.txt\n@@ -1 +1 @@\n-c\n+d\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("first.txt"));
        assert_eq!(p.new.as_deref(), Some("first.txt"));
    }

    #[test]
    fn trailing_spaces_preserved() {
        let diff = "diff --git a/foo  b/foo \n--- a/foo \n+++ b/foo \n@@ -1 +1 @@\n-old\n+new\n";
        let p = paths_from_diff(diff).expect("paths");
        assert_eq!(p.old.as_deref(), Some("foo "));
        assert_eq!(p.new.as_deref(), Some("foo "));
    }

    #[test]
    fn new_path_from_single_line() {
        assert_eq!(
            new_path_from_header("diff --git a/foo.txt b/foo.txt"),
            Some("foo.txt".to_string())
        );
        assert_eq!(
            new_path_from_header("diff --git \"a/foo\\nbar\" \"b/foo\\nbar\""),
            Some("foo\nbar".to_string())
        );
    }

    #[test]
    fn binary_dev_null_sides() {
        let added = "diff --git a/new.bin b/new.bin\nBinary files /dev/null and b/new.bin differ\n";
        let p = paths_from_diff(added).expect("paths");
        assert_eq!(p.old, None);
        assert_eq!(p.new.as_deref(), Some("new.bin"));

        let deleted =
            "diff --git a/old.bin b/old.bin\nBinary files a/old.bin and /dev/null differ\n";
        let p = paths_from_diff(deleted).expect("paths");
        assert_eq!(p.old.as_deref(), Some("old.bin"));
        assert_eq!(p.new, None);
    }
}
