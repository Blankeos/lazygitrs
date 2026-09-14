//! Ctrl-F diff-content grep: a synchronous [`PopupState::ListPicker`] dialog.
//!
//! Stays in the originating context — the Files worktree list, the
//! CommitFiles/StashFiles/BranchCommitFiles list, or the compare `diff_mode`
//! view. Ctrl-F behaves like a second `/` search but over diff contents:
//! every hunk line (`+`/`-`/context) becomes a filterable row, typing narrows
//! via the existing `list_picker_*` token-AND helpers (order-free,
//! case-insensitive), and Enter selects that file in the current list
//! (no context switch, no borrowed screen).
//!
//! No new deps, no threads.

use std::collections::HashSet;

use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::gui::Gui;
use crate::gui::context::ContextId;
use crate::gui::popup::{ListPickerItem, MessageKind, PopupState};

/// Max grep candidate rows built synchronously; the dialog title notes when
/// the build stopped early at this cap.
pub const DIFF_GREP_MAX_ITEMS: usize = 5000;
/// Untracked files larger than this are skipped in the Files scope synthesis.
const MAX_UNTRACKED_BYTES: u64 = 1_000_000;
/// Per-row display text width (chars); matching covers the truncated label.
const MAX_TEXT_CHARS: usize = 200;
/// Separator inside ListPicker values (never appears in file paths).
const SEP: char = '\x1f';

/// Ctrl-F, the grep-dialog shortcut.
pub fn is_diff_grep_key(key: KeyEvent) -> bool {
    matches!(key.code, KeyCode::Char('f') | KeyCode::Char('F'))
        && key.modifiers.contains(KeyModifiers::CONTROL)
}

/// Scope captured at dialog-open time. Confirm re-resolves the file by path
/// against the live list, so a background refresh between open and Enter
/// degrades to a no-op instead of selecting the wrong file.
#[derive(Debug, Clone, Copy)]
enum DiffGrepScope {
    Worktree,
    CommitFiles { ctx: ContextId },
    Compare,
}

/// Detect the scope from the current context, build grep rows synchronously
/// from existing git diff plumbing, and open the [`PopupState::ListPicker`]
/// dialog (empty free-entry category = confirm real matches only).
pub fn open_diff_grep_picker(gui: &mut Gui) -> Result<()> {
    let scope: DiffGrepScope;
    let scope_title: String;
    let diff_text: String;
    // Untracked worktree files have no `git diff` output; their content lines
    // are appended after the tracked diff below.
    let mut untracked_paths: Vec<String> = Vec::new();

    if gui.diff_mode.active {
        if !gui.diff_mode.has_both_refs() {
            gui.popup = PopupState::Message {
                title: "Grep diff".to_string(),
                message: "Set both A and B refs first.".to_string(),
                kind: MessageKind::Info,
            };
            return Ok(());
        }
        let ref_a = gui.diff_mode.ref_a.clone();
        let ref_b = gui.diff_mode.ref_b.clone();
        diff_text = gui
            .git
            .diff_refs_paths(&ref_a, &ref_b, &[])
            .unwrap_or_default();
        scope_title = format!("Grep {ref_a}..{ref_b}");
        scope = DiffGrepScope::Compare;
    } else {
        match gui.context_mgr.active() {
            ContextId::CommitFiles | ContextId::StashFiles | ContextId::BranchCommitFiles => {
                let ctx = gui.context_mgr.active();
                let hash = gui.commit_files_hash.clone();
                if hash.is_empty() {
                    gui.popup = PopupState::Message {
                        title: "Grep diff".to_string(),
                        message: "No commit selected.".to_string(),
                        kind: MessageKind::Info,
                    };
                    return Ok(());
                }
                diff_text = gui.git.diff_commit(&hash).unwrap_or_default();
                let short: String = hash.chars().take(7).collect();
                scope_title = format!("Grep {short}");
                scope = DiffGrepScope::CommitFiles { ctx };
            }
            ContextId::Files => {
                // `git diff HEAD` covers staged+unstaged tracked changes; on
                // unborn HEAD (or any error) fall back to just untracked text.
                diff_text = gui.git.diff_all().unwrap_or_default();
                let model = gui.model.lock().unwrap();
                untracked_paths = model
                    .files
                    .iter()
                    .filter(|f| !f.tracked)
                    .map(|f| f.current_path().to_string())
                    .collect();
                drop(model);
                scope_title = "Grep worktree diff".to_string();
                scope = DiffGrepScope::Worktree;
            }
            _ => return Ok(()),
        }
    }

    let mut items: Vec<ListPickerItem> = Vec::new();
    let mut truncated = parse_unified_diff_items(&diff_text, &mut items, DIFF_GREP_MAX_ITEMS);

    for path in &untracked_paths {
        if items.len() >= DIFF_GREP_MAX_ITEMS {
            truncated = true;
            break;
        }
        let abs = gui.git.repo_path().join(path);
        if std::fs::metadata(&abs).map(|m| m.len()).unwrap_or(0) > MAX_UNTRACKED_BYTES {
            continue;
        }
        // Empty covers binary/unreadable/empty files — all skipped per spec.
        let content = gui.git.file_content(path).unwrap_or_default();
        if content.is_empty() {
            continue;
        }
        for (i, line) in content.lines().enumerate() {
            if items.len() >= DIFF_GREP_MAX_ITEMS {
                truncated = true;
                break;
            }
            push_grep_item(&mut items, path, i + 1, '+', "new", line);
        }
    }

    if items.is_empty() {
        gui.popup = PopupState::Message {
            title: "Grep diff".to_string(),
            message: "No diff content to search.".to_string(),
            kind: MessageKind::Info,
        };
        return Ok(());
    }

    let title = if truncated {
        format!("{scope_title} (first {DIFF_GREP_MAX_ITEMS} lines — keep typing to filter)")
    } else {
        format!("{scope_title} ({} lines)", items.len())
    };
    gui.show_list_picker(
        title,
        items,
        String::new(),
        Box::new(move |gui, value| confirm_diff_grep(gui, scope, value)),
    );
    Ok(())
}

/// Enter handler: parse the row value and select that file in the originating
/// list (no context switch). Unknown values or missing files no-op.
fn confirm_diff_grep(gui: &mut Gui, scope: DiffGrepScope, value: &str) -> Result<()> {
    let Some((path, _line)) = parse_grep_value(value) else {
        return Ok(());
    };
    match scope {
        DiffGrepScope::Worktree => {
            let idx = {
                let model = gui.model.lock().unwrap();
                model
                    .files
                    .iter()
                    .position(|f| f.current_path() == path || f.name == path)
            };
            let Some(idx) = idx else { return Ok(()) };
            if gui.show_file_tree {
                expand_ancestors(&mut gui.collapsed_dirs, &path);
                gui.update_file_tree_state();
                let pos = gui
                    .file_tree_nodes
                    .iter()
                    .position(|n| n.file_index == Some(idx));
                let Some(pos) = pos else { return Ok(()) };
                gui.context_mgr.set_selection(pos);
            } else {
                gui.context_mgr.set_selection_for(ContextId::Files, idx);
            }
            gui.context_mgr.viewport_manually_scrolled = false;
            gui.needs_diff_refresh = true;
        }
        DiffGrepScope::CommitFiles { ctx } => {
            let idx = {
                let model = gui.model.lock().unwrap();
                model
                    .commit_files
                    .iter()
                    .position(|f| f.current_path() == path || f.name == path)
            };
            let Some(idx) = idx else { return Ok(()) };
            if gui.show_commit_file_tree {
                expand_ancestors(&mut gui.commit_files_collapsed_dirs, &path);
                super::commit_files::update_commit_file_tree_state(gui);
                let pos = gui
                    .commit_file_tree_nodes
                    .iter()
                    .position(|n| n.file_index == Some(idx));
                let Some(pos) = pos else { return Ok(()) };
                gui.context_mgr.set_selection_for(ctx, pos);
            } else {
                gui.context_mgr.set_selection_for(ctx, idx);
            }
            gui.context_mgr.viewport_manually_scrolled = false;
            gui.needs_diff_refresh = true;
        }
        DiffGrepScope::Compare => {
            let idx = gui
                .diff_mode
                .diff_files
                .iter()
                .position(|f| f.current_path() == path || f.name == path);
            let Some(idx) = idx else { return Ok(()) };
            if gui.diff_mode.show_tree {
                expand_ancestors(&mut gui.diff_mode.collapsed_dirs, &path);
                let files = gui.diff_mode.diff_files.clone();
                gui.diff_mode.tree_nodes = crate::model::file_tree::build_commit_file_tree(
                    &files,
                    &gui.diff_mode.collapsed_dirs,
                );
                let pos = gui
                    .diff_mode
                    .tree_nodes
                    .iter()
                    .position(|n| n.file_index == Some(idx));
                let Some(pos) = pos else { return Ok(()) };
                gui.diff_mode.diff_files_selected = pos;
            } else {
                gui.diff_mode.diff_files_selected = idx;
            }
            gui.diff_mode.viewport_manually_scrolled = false;
            gui.needs_diff_refresh = true;
        }
    }
    Ok(())
}

/// Expand collapsed tree ancestors of `path` (plus the root) so a jumped-to
/// file node is visible after the tree rebuild.
fn expand_ancestors(collapsed: &mut HashSet<String>, path: &str) {
    collapsed.remove(".");
    let mut prefix = String::new();
    let parts: Vec<&str> = path.split('/').collect();
    for (i, part) in parts.iter().enumerate() {
        if i + 1 >= parts.len() {
            break; // the file itself, not a directory
        }
        if !prefix.is_empty() {
            prefix.push('/');
        }
        prefix.push_str(part);
        collapsed.remove(&prefix);
    }
}

fn push_grep_item(
    items: &mut Vec<ListPickerItem>,
    path: &str,
    line_no: usize,
    marker: char,
    side: &str,
    text: &str,
) {
    let text = text.strip_suffix('\r').unwrap_or(text);
    let short: String = text.chars().take(MAX_TEXT_CHARS).collect();
    items.push(ListPickerItem {
        value: format!("{path}{SEP}{line_no}{SEP}{side}"),
        label: format!("{path}:{line_no} {marker} {short}"),
        category: path.to_string(),
        description: None,
    });
}

/// Decode an Enter value back to `(path, line_no)`; `None` = unknown value.
pub fn parse_grep_value(value: &str) -> Option<(String, usize)> {
    let mut parts = value.split(SEP);
    let path = parts.next()?.to_string();
    let line: usize = parts.next()?.parse().ok()?;
    if path.is_empty() || line == 0 {
        return None;
    }
    Some((path, line))
}

/// Parse unified-diff hunk body lines into grep rows. Tracks the current file
/// from `diff --git` headers and line numbers from `@@` headers; skips diff
/// metadata, binary content, and files that never open a hunk. Returns true
/// when `cap` stopped the build early.
pub fn parse_unified_diff_items(diff: &str, items: &mut Vec<ListPickerItem>, cap: usize) -> bool {
    let mut cur: Option<String> = None;
    let mut old_no: usize = 0;
    let mut new_no: usize = 0;
    let mut in_hunk = false;
    let mut binary = false;

    for raw in diff.lines() {
        if items.len() >= cap {
            return true;
        }
        if let Some(rest) = raw.strip_prefix("diff --git ") {
            cur = diff_git_new_path(rest);
            in_hunk = false;
            binary = false;
            continue;
        }
        // `--- a/…` / `+++ b/…` headers override the `diff --git` path. They
        // carry spaces intact, fixing ambiguous unquoted headers, and handle
        // renames/deletes (`+++ /dev/null` keeps the old path).
        if let Some(rest) = raw.strip_prefix("--- ") {
            if let Some(p) = diff_header_path(rest) {
                cur = Some(p);
            }
            in_hunk = false;
            continue;
        }
        if let Some(rest) = raw.strip_prefix("+++ ") {
            if rest.trim() == "/dev/null" {
                // Deleted file: keep the `---` (old) path in `cur`.
            } else if let Some(p) = diff_header_path(rest) {
                cur = Some(p);
            }
            in_hunk = false;
            continue;
        }
        // `GIT binary patch` opens a multi-line literal block lasting until
        // the next file header; `Binary files … differ` is a single line.
        if binary {
            continue;
        }
        if raw.starts_with("GIT binary patch") {
            binary = true;
            in_hunk = false;
            continue;
        }
        if raw.starts_with("Binary files ") {
            in_hunk = false;
            continue;
        }
        if raw.starts_with("@@") {
            if let Some((old_start, new_start)) = parse_hunk_header(raw) {
                old_no = old_start;
                new_no = new_start;
                in_hunk = cur.is_some();
            } else {
                in_hunk = false;
            }
            continue;
        }
        if !in_hunk {
            continue;
        }
        let Some(path) = cur.as_deref() else {
            in_hunk = false;
            continue;
        };
        if raw.is_empty() {
            // Blank context line.
            push_grep_item(items, path, new_no, ' ', "new", "");
            old_no += 1;
            new_no += 1;
            continue;
        }
        match raw.as_bytes()[0] {
            // Byte dispatch (not `starts_with("+++")`): an added line whose
            // content starts with `+` must stay a body line — `+++ b/…`
            // headers only occur outside hunks, where `in_hunk` is false.
            b'+' => {
                push_grep_item(items, path, new_no, '+', "new", &raw[1..]);
                new_no += 1;
            }
            b'-' => {
                push_grep_item(items, path, old_no, '-', "old", &raw[1..]);
                old_no += 1;
            }
            b' ' => {
                push_grep_item(items, path, new_no, ' ', "new", &raw[1..]);
                old_no += 1;
                new_no += 1;
            }
            // "\ No newline at end of file".
            b'\\' => {}
            _ => in_hunk = false,
        }
    }
    false
}

/// `@@ -old[,len] +new[,len] @@ …` → `(old_start, new_start)`.
fn parse_hunk_header(line: &str) -> Option<(usize, usize)> {
    let rest = line.strip_prefix("@@")?;
    let (ranges, _) = rest.split_once("@@")?;
    let mut it = ranges.split_whitespace();
    let old = it.next()?.strip_prefix('-')?;
    let new = it.next()?.strip_prefix('+')?;
    let old_start: usize = old.split(',').next()?.parse().ok()?;
    let new_start: usize = new.split(',').next()?.parse().ok()?;
    Some((old_start, new_start))
}

/// Current (b-side) path from a `diff --git` header, or the a-side path for
/// deletions (`b/…` is `/dev/null`). Handles renames (`old -> new` labels
/// never appear here — headers always carry both raw paths).
fn diff_git_new_path(rest: &str) -> Option<String> {
    let (a, b) = split_git_paths(rest.trim())?;
    let a = unquote_diff_path(&a);
    let b = unquote_diff_path(&b);
    let raw = if b == "/dev/null" { a } else { b };
    let stripped = raw
        .strip_prefix("b/")
        .or_else(|| raw.strip_prefix("a/"))
        .unwrap_or(&raw);
    Some(stripped.to_string())
}

/// Path from a `--- a/…` / `+++ b/…` header. These carry the full path with
/// spaces intact (no split needed), so they repair the ambiguous unquoted
/// `diff --git a/my file b/my file` case. Returns `None` for `/dev/null`.
fn diff_header_path(line: &str) -> Option<String> {
    let raw = unquote_diff_path(line.trim());
    if raw == "/dev/null" {
        return None;
    }
    let stripped = raw
        .strip_prefix("b/")
        .or_else(|| raw.strip_prefix("a/"))
        .unwrap_or(&raw);
    Some(stripped.to_string())
}

/// Split `a/old b/new`, where either side may be C-quoted (spaces inside).
fn split_git_paths(s: &str) -> Option<(String, String)> {
    if let Some(after_open) = s.strip_prefix('"') {
        let end = find_closing_quote(after_open)?;
        let first = &s[..end + 2]; // include both quotes
        let second = after_open[end + 1..].trim_start().to_string();
        if second.is_empty() {
            return None;
        }
        Some((first.to_string(), second))
    } else {
        let mut it = s.splitn(2, ' ');
        Some((it.next()?.to_string(), it.next()?.to_string()))
    }
}

/// Byte offset (relative to `s`, which excludes the opening quote) of the
/// closing quote, skipping `\X` escapes.
fn find_closing_quote(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'\\' {
            i += 2;
            continue;
        }
        if bytes[i] == b'"' {
            return Some(i);
        }
        i += 1;
    }
    None
}

/// Reverse git's C-style quoting of non-ASCII paths (`"a/\303\241.txt"`).
/// Octal escapes decode to bytes (UTF-8 sequences), so this is byte-based.
fn unquote_diff_path(s: &str) -> String {
    if !(s.len() >= 2 && s.starts_with('"') && s.ends_with('"')) {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let inner = &bytes[1..bytes.len() - 1];
    let mut out: Vec<u8> = Vec::with_capacity(inner.len());
    let mut i = 0;
    while i < inner.len() {
        if inner[i] != b'\\' || i + 1 >= inner.len() {
            out.push(inner[i]);
            i += 1;
            continue;
        }
        match inner[i + 1] {
            b'"' => {
                out.push(b'"');
                i += 2;
            }
            b'\\' => {
                out.push(b'\\');
                i += 2;
            }
            b'n' => {
                out.push(b'\n');
                i += 2;
            }
            b't' => {
                out.push(b'\t');
                i += 2;
            }
            c @ b'0'..=b'7' => {
                let mut val = (c - b'0') as u32;
                let mut j = i + 2;
                for _ in 0..2 {
                    if j < inner.len() && inner[j] >= b'0' && inner[j] < b'8' {
                        val = val * 8 + (inner[j] - b'0') as u32;
                        j += 1;
                    } else {
                        break;
                    }
                }
                out.push(val as u8);
                i = j;
            }
            _ => {
                out.push(inner[i + 1]);
                i += 2;
            }
        }
    }
    String::from_utf8(out).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(items: &[ListPickerItem]) -> Vec<String> {
        items.iter().map(|i| i.label.clone()).collect()
    }

    #[test]
    fn parses_hunk_lines_with_new_side_numbers() {
        let diff = "diff --git a/f.txt b/f.txt\n\
            --- a/f.txt\n+++ b/f.txt\n\
            @@ -1,3 +1,3 @@\n ctx\n-old\n+new\n more\n";
        let mut items = Vec::new();
        let truncated = parse_unified_diff_items(diff, &mut items, 5000);
        assert!(!truncated);
        assert_eq!(
            labels(&items),
            vec![
                "f.txt:1   ctx",
                "f.txt:2 - old",
                "f.txt:2 + new",
                "f.txt:3   more",
            ]
        );
        // Values resolve back to (path, line).
        assert_eq!(
            parse_grep_value(&items[2].value),
            Some(("f.txt".to_string(), 2))
        );
        // Added-line content starting with `+` is not a file header.
        let diff = "diff --git a/g b/g\n--- a/g\n+++ b/g\n@@ -1 +1 @@\n+++plus\n";
        let mut items = Vec::new();
        parse_unified_diff_items(diff, &mut items, 5000);
        assert_eq!(labels(&items), vec!["g:1 + ++plus"]);
    }

    #[test]
    fn rename_uses_new_path_and_delete_uses_old_path() {
        let diff = "diff --git a/old.txt b/new.txt\n\
            similarity index 90%\nrename from old.txt\nrename to new.txt\n\
            --- a/old.txt\n+++ b/new.txt\n\
            @@ -1 +1 @@\n-x\n+y\n";
        let mut items = Vec::new();
        parse_unified_diff_items(diff, &mut items, 5000);
        assert_eq!(labels(&items), vec!["new.txt:1 - x", "new.txt:1 + y"]);

        let diff = "diff --git a/gone.txt b/gone.txt\n\
            deleted file mode 100644\n--- a/gone.txt\n+++ /dev/null\n\
            @@ -1,2 +0,0 @@\n-aaa\n-bbb\n";
        let mut items = Vec::new();
        parse_unified_diff_items(diff, &mut items, 5000);
        assert_eq!(labels(&items), vec!["gone.txt:1 - aaa", "gone.txt:2 - bbb"]);
    }

    #[test]
    fn binary_and_metadata_lines_are_skipped() {
        let diff = "diff --git a/bin.png b/bin.png\n\
            new file mode 100644\nindex 0000000..1111111\n\
            Binary files /dev/null and b/bin.png differ\n\
            diff --git a/blob.bin b/blob.bin\n\
            GIT binary patch\nliteral 3\nzc$@)x0|6E4\n\n\
            literal 0\nHcmZQ00000\n\
            diff --git a/f.txt b/f.txt\n\
            --- a/f.txt\n+++ b/f.txt\n@@ -1 +1 @@\n-a\n+b\n";
        let mut items = Vec::new();
        parse_unified_diff_items(diff, &mut items, 5000);
        assert_eq!(labels(&items), vec!["f.txt:1 - a", "f.txt:1 + b"]);
    }

    #[test]
    fn quoted_paths_are_unquoted() {
        let diff = "diff --git \"a/sp ace.txt\" \"b/sp ace.txt\"\n\
            --- \"a/sp ace.txt\"\n+++ \"b/sp ace.txt\"\n@@ -1 +1 @@\n-a\n+b\n";
        let mut items = Vec::new();
        parse_unified_diff_items(diff, &mut items, 5000);
        assert_eq!(labels(&items), vec!["sp ace.txt:1 - a", "sp ace.txt:1 + b"]);
        // Octal-escaped UTF-8 ("\303\241" = á).
        assert_eq!(unquote_diff_path("\"\\303\\241.txt\""), "á.txt");
    }

    #[test]
    fn cap_truncates_the_build() {
        let mut diff = String::new();
        for i in 0..10 {
            diff.push_str(&format!(
                "diff --git a/f{i}.txt b/f{i}.txt\n--- a/f{i}.txt\n+++ b/f{i}.txt\n@@ -1 +1 @@\n-a\n+b\n"
            ));
        }
        let mut items = Vec::new();
        let truncated = parse_unified_diff_items(&diff, &mut items, 5);
        assert!(truncated);
        assert_eq!(items.len(), 5);
    }

    #[test]
    fn grep_value_rejects_garbage() {
        assert_eq!(
            parse_grep_value(&format!("a/b.rs{SEP}12{SEP}new")),
            Some(("a/b.rs".to_string(), 12))
        );
        assert!(parse_grep_value("").is_none());
        assert!(parse_grep_value("no-separators").is_none());
        // Colons in paths are safe: the separator is \x1f, not ':'.
        assert_eq!(
            parse_grep_value(&format!("we:ird.rs{SEP}3{SEP}old")),
            Some(("we:ird.rs".to_string(), 3))
        );
        assert!(parse_grep_value(&format!("a.rs{SEP}0{SEP}new")).is_none());
        assert!(parse_grep_value(&format!("a.rs{SEP}abc{SEP}new")).is_none());
        assert!(parse_grep_value(&format!("{SEP}4{SEP}new")).is_none());
    }
}
