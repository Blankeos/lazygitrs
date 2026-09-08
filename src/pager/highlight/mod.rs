pub mod config;
pub mod dynamic;

use std::collections::HashMap;
use std::path::PathBuf;

use ratatui::prelude::*;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

use crate::config::Theme;

use config::{HIGHLIGHT_NAMES, STATIC_LANGS};

/// Map a highlight index to a ratatui Color using the active theme.
/// Prefix-matches so upstream captures (`keyword.function`, `markup.heading`,
/// `string.special.symbol`, …) work without touching this function.
pub fn highlight_color(index: usize, theme: &Theme) -> Color {
    let name = HIGHLIGHT_NAMES.get(index).copied().unwrap_or("");
    match name {
        "function.macro" => theme.syntax_function_macro,
        "variable.builtin" => theme.syntax_variable_builtin,
        "variable.member" | "property" => theme.syntax_variable_member,
        "tag.attribute" => theme.syntax_attribute,
        _ => {
            let base = name.split('.').next().unwrap_or(name);
            match base {
                "comment" => theme.syntax_comment,
                "keyword" => theme.syntax_keyword,
                "string" => theme.syntax_string,
                "number" | "constant" | "boolean" => theme.syntax_number,
                "function" => theme.syntax_function,
                "type" | "constructor" => theme.syntax_type,
                "module" | "namespace" => theme.syntax_module,
                "operator" => theme.syntax_operator,
                "tag" => theme.syntax_tag,
                "attribute" => theme.syntax_attribute,
                "label" => theme.syntax_label,
                "punctuation" => theme.syntax_punctuation,
                "markup" if name.contains("heading") => theme.syntax_keyword,
                "markup"
                    if name.contains("link") || name.contains("url") || name.contains("raw") =>
                {
                    theme.syntax_string
                }
                "markup" if name.contains("quote") => theme.syntax_comment,
                "diff" if name.contains("plus") => theme.syntax_string,
                "diff" if name.contains("minus") => theme.syntax_keyword,
                "diff" => theme.syntax_type,
                "error" => theme.syntax_keyword,
                _ => theme.syntax_default,
            }
        }
    }
}

fn first_line(content: &str) -> Option<&str> {
    let line = content.lines().next().unwrap_or("");
    if line.is_empty() { None } else { Some(line) }
}

/// Static (built-in) → dynamic (`~/.config/lazygitrs/syntax/`) → plain text.
fn config_for_file(filename: &str, content: &str) -> Option<&'static HighlightConfiguration> {
    let first = first_line(content);
    if let Some((_, c)) = config::config_for_file(filename, first) {
        return Some(c);
    }
    dynamic::get_config_for_file(filename, first)
}

/// Compile static queries + prepare the dynamic folder, off the critical path.
/// Called from a background thread (see `Gui::new`).
pub fn warm_configs() {
    config::warm_all();
    let _ = dynamic::ensure_skeleton();
    dynamic::preload();
}

/// One row of the `?` → "Syntax highlighting..." menu.
/// Plain data (no widget types) so it stays unit-testable.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SyntaxRow {
    /// Stable id the menu confirms with: a known language id (`swift`),
    /// `static:<id>` for built-ins, or `__reload__` for the rescan row.
    pub value: String,
    /// What the row shows, e.g. `✓ Rust`, `○ Swift`.
    pub label: String,
    /// Group header: `Ready` or `Available` (empty for the rescan row).
    pub category: String,
    /// Right-side note, e.g. `Ready`, `Enter to install`.
    pub description: String,
}

pub const SYNTAX_RELOAD_VALUE: &str = "__reload__";

/// Rows for the syntax menu: built-ins + installed extras under `Ready`,
/// missing/broken extras under `Available`, rescan row last.
/// Alphabetical within each group; pretty display names, no ids or paths.
pub fn syntax_menu_rows() -> Vec<SyntaxRow> {
    use config::pretty_name;

    let mut ready_static: Vec<&str> = STATIC_LANGS
        .iter()
        .filter(|e| (e.get)().is_some())
        .map(|e| e.name)
        .collect();
    ready_static.sort_by_key(|n| pretty_name(n).to_string());

    let mut ready_extra: Vec<&str> = Vec::new();
    let mut available: Vec<SyntaxRow> = Vec::new();
    for (id, _exts) in dynamic::known_langs() {
        match dynamic::install_state(id) {
            dynamic::InstallState::Installed => ready_extra.push(id),
            dynamic::InstallState::Missing => available.push(SyntaxRow {
                value: id.to_string(),
                label: format!("○ {}", pretty_name(id)),
                category: "Available".to_string(),
                description: "Enter to install".to_string(),
            }),
            dynamic::InstallState::Failed(_) => available.push(SyntaxRow {
                value: id.to_string(),
                label: format!("! {}", pretty_name(id)),
                category: "Available".to_string(),
                description: "Enter to retry".to_string(),
            }),
        }
    }
    ready_extra.sort_by_key(|n| pretty_name(n).to_string());
    available.sort_by(|a, b| a.label.cmp(&b.label));

    let mut rows: Vec<SyntaxRow> = ready_static
        .iter()
        .map(|n| SyntaxRow {
            value: format!("static:{n}"),
            label: format!("✓ {}", pretty_name(n)),
            category: "Ready".to_string(),
            description: "Ready".to_string(),
        })
        .collect();
    rows.extend(ready_extra.iter().map(|n| SyntaxRow {
        value: n.to_string(),
        label: format!("✓ {}", pretty_name(n)),
        category: "Ready".to_string(),
        description: "Ready".to_string(),
    }));
    rows.extend(available);
    // User-added grammars (via the folder) that aren't one of the 5
    // installable extras: show them as Ready under their own name.
    let (dyn_langs, _) = dynamic::installed();
    let mut custom: Vec<&String> = dyn_langs
        .iter()
        .filter(|name| !dynamic::known_langs().iter().any(|(k, _)| k == name))
        .collect();
    custom.sort();
    rows.extend(custom.iter().map(|name| SyntaxRow {
        value: format!("custom:{name}"),
        label: format!("✓ {}", pretty_name(name)),
        category: "Ready".to_string(),
        description: "Added by you".to_string(),
    }));
    rows.push(SyntaxRow {
        value: SYNTAX_RELOAD_VALUE.to_string(),
        label: "Check again".to_string(),
        category: String::new(),
        description: "Rescan folder".to_string(),
    });
    rows
}

/// What confirming a menu row value means. Pure + tested.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SyntaxConfirm {
    /// Already working — show a nothing-to-do note.
    AlreadyReady(String),
    /// Needs download + build on a background thread.
    Install { id: String, pretty: String },
    /// Rescan the folder, then reopen the menu.
    Reload,
}

pub fn syntax_confirm_for(value: &str) -> SyntaxConfirm {
    use config::pretty_name;
    if value == SYNTAX_RELOAD_VALUE {
        return SyntaxConfirm::Reload;
    }
    // User-added (folder) and built-in languages are already working.
    if let Some(id) = value.strip_prefix("static:") {
        let pretty = pretty_name(id).to_string();
        return SyntaxConfirm::AlreadyReady(pretty);
    }
    if let Some(name) = value.strip_prefix("custom:") {
        let label = pretty_name(name);
        let pretty = if label == "Unknown" {
            name.to_string()
        } else {
            label.to_string()
        };
        return SyntaxConfirm::AlreadyReady(pretty);
    }
    match dynamic::install_state(value) {
        dynamic::InstallState::Installed => {
            SyntaxConfirm::AlreadyReady(pretty_name(value).to_string())
        }
        dynamic::InstallState::Missing | dynamic::InstallState::Failed(_) => {
            SyntaxConfirm::Install {
                id: value.to_string(),
                pretty: pretty_name(value).to_string(),
            }
        }
    }
}

/// Pre-computed highlights for an entire file, organized by line number.
/// Handles multi-line constructs like JSDoc comments properly.
#[derive(Default)]
pub struct FileHighlighter {
    line_highlights: HashMap<usize, Vec<(String, Option<usize>)>>,
}

impl FileHighlighter {
    pub fn new(content: &str, filename: &str) -> Self {
        let Some(lang_config) = config_for_file(filename, content) else {
            return Self::default();
        };

        let mut highlighter = Highlighter::new();
        let highlights =
            highlighter.highlight(lang_config, content.as_bytes(), None, None, |_| None);

        let Ok(highlights) = highlights else {
            return Self::default();
        };

        // Build byte offset -> line number map (1-based)
        let mut line_starts: Vec<usize> = vec![0];
        for (i, c) in content.char_indices() {
            if c == '\n' {
                line_starts.push(i + 1);
            }
        }

        let byte_to_line = |byte_offset: usize| -> usize {
            match line_starts.binary_search(&byte_offset) {
                Ok(line) => line + 1,
                Err(line) => line,
            }
        };

        let mut line_highlights: HashMap<usize, Vec<(String, Option<usize>)>> = HashMap::new();
        let mut current_highlight: Option<usize> = None;

        for event in highlights.flatten() {
            match event {
                HighlightEvent::Source { start, end } => {
                    let text = &content[start..end];
                    let start_line = byte_to_line(start);
                    let mut current_line = start_line;
                    let mut line_start = 0;

                    for (i, c) in text.char_indices() {
                        if c == '\n' {
                            let line_text = &text[line_start..i];
                            if !line_text.is_empty() {
                                line_highlights
                                    .entry(current_line)
                                    .or_default()
                                    .push((line_text.to_string(), current_highlight));
                            }
                            current_line += 1;
                            line_start = i + 1;
                        }
                    }

                    if line_start < text.len() {
                        let remaining = &text[line_start..];
                        line_highlights
                            .entry(current_line)
                            .or_default()
                            .push((remaining.to_string(), current_highlight));
                    }
                }
                HighlightEvent::HighlightStart(h) => {
                    current_highlight = Some(h.0);
                }
                HighlightEvent::HighlightEnd => {
                    current_highlight = None;
                }
            }
        }

        Self { line_highlights }
    }

    /// Get highlighted spans for a specific line (1-based line number).
    pub fn get_line_spans<'a>(
        &self,
        line_number: usize,
        bg: Option<Color>,
        theme: &Theme,
    ) -> Vec<Span<'a>> {
        let bg_color = bg.unwrap_or(Color::Reset);
        let default_fg = theme.syntax_default;

        self.line_highlights
            .get(&line_number)
            .map(|spans| {
                spans
                    .iter()
                    .filter(|(text, _)| *text != "\n")
                    .map(|(text, highlight_idx)| {
                        let fg = highlight_idx
                            .map(|i| highlight_color(i, theme))
                            .unwrap_or(default_fg);
                        Span::styled(text.clone(), Style::default().fg(fg).bg(bg_color))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod highlight_tests {
    use super::*;

    #[test]
    fn all_static_configs_compile() {
        for e in STATIC_LANGS {
            assert!((e.get)().is_some(), "static grammar failed: {}", e.name);
        }
    }

    #[test]
    fn rust_snippet_highlights() {
        let theme = Theme::dark();
        let hl = FileHighlighter::new("fn main() {\n    // hi\n    let x = \"s\";\n}\n", "main.rs");
        assert!(!hl.get_line_spans(1, None, &theme).is_empty());
        assert!(!hl.get_line_spans(2, None, &theme).is_empty());
    }

    #[test]
    fn unknown_extension_is_plain() {
        let theme = Theme::dark();
        let hl = FileHighlighter::new("hello", "file.zzzunknown");
        assert!(hl.get_line_spans(1, None, &theme).is_empty());
    }

    /// End-to-end: parse + highlight a snippet per static language. Catches
    /// parser ABI mismatches, not just query-compile failures.
    /// (Heavy grammars — swift, c-sharp, c++, php, ruby — are dynamic and
    /// covered by `dynamic::known_defaults` tests instead.)
    #[test]
    fn new_languages_highlight_end_to_end() {
        let theme = Theme::dark();
        let cases = [
            ("main.c", "int main() {\n    return 0;\n}\n"),
            (
                "A.java",
                "class A {\n    public static void main(String[] a) {}\n}\n",
            ),
            ("Dockerfile", "FROM ubuntu:22.04\nRUN apt-get update\n"),
            ("Dockerfile.dev", "FROM ubuntu:22.04\n"),
            ("a.lua", "local x = 1 -- comment\n"),
            ("a.yml", "key: value\nlist:\n  - one\n"),
        ];
        for (file, content) in cases {
            let line1 = content.lines().next().unwrap_or("");
            let hl = FileHighlighter::new(content, file);
            assert!(
                !hl.get_line_spans(1, None, &theme).is_empty(),
                "{file} ({line1:?}) produced no highlights"
            );
        }
    }

    #[test]
    fn heavy_languages_fall_through_without_grammars() {
        // No .so in the test env → plain text, no panic.
        let theme = Theme::dark();
        for file in ["a.swift", "A.cs", "main.cpp", "a.php", "a.rb"] {
            let hl = FileHighlighter::new("hello world", file);
            assert!(
                hl.get_line_spans(1, None, &theme).is_empty(),
                "{file} should be plain without its grammar installed"
            );
        }
    }
}

#[cfg(test)]
mod syntax_menu_tests {
    use super::*;

    fn joined(rows: &[SyntaxRow], field: impl Fn(&SyntaxRow) -> &str) -> String {
        rows.iter().map(field).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn rows_cover_every_language_exactly_once() {
        let rows = syntax_menu_rows();
        let mut values: Vec<&str> = rows.iter().map(|r| r.value.as_str()).collect();
        values.sort();
        values.dedup();
        assert_eq!(values.len(), rows.len(), "duplicate row values");

        // Every static language + every installable extra + rescan row.
        let mut ids: Vec<String> = STATIC_LANGS
            .iter()
            .map(|e| format!("static:{}", e.name))
            .collect();
        ids.extend(dynamic::known_langs().iter().map(|(id, _)| id.to_string()));
        ids.push(SYNTAX_RELOAD_VALUE.to_string());
        for id in &ids {
            assert!(
                rows.iter().any(|r| &r.value == id),
                "missing row for {id}\n{}",
                joined(&rows, |r| r.value.as_str())
            );
        }
        // Installed extras confirm by id (AlreadyReady), not as static.
        for (id, _) in dynamic::known_langs() {
            if matches!(dynamic::install_state(id), dynamic::InstallState::Installed) {
                assert!(rows.iter().any(|r| r.value == id));
            }
        }
    }

    #[test]
    fn rows_use_plain_language() {
        let rows = syntax_menu_rows();
        let text =
            joined(&rows, |r| r.label.as_str()) + "\n" + &joined(&rows, |r| r.description.as_str());
        for banned in [
            "c-sharp",
            "cpp",
            "tsx",
            ".so",
            ".scm",
            "grammars/",
            "queries/",
            "scripts/",
            "nvim",
            "helix",
            "QueryError",
            "language-server",
            "LSP",
        ] {
            assert!(!text.contains(banned), "row leaks jargon: {banned}\n{text}");
        }
        assert!(text.contains("Swift"), "missing Swift row:\n{text}");
        assert!(text.contains("C#"), "missing C# row:\n{text}");
        assert!(text.contains("C++"), "missing C++ row:\n{text}");
    }

    #[test]
    fn confirm_mapping() {
        assert_eq!(
            syntax_confirm_for(SYNTAX_RELOAD_VALUE),
            SyntaxConfirm::Reload
        );
        assert_eq!(
            syntax_confirm_for("static:rust"),
            SyntaxConfirm::AlreadyReady("Rust".to_string())
        );
        // Unknown ids can never be installed in the test env — deterministic
        // regardless of what the developer has in their real syntax folder.
        assert_eq!(
            syntax_confirm_for("klingon"),
            SyntaxConfirm::Install {
                id: "klingon".to_string(),
                pretty: "Unknown".to_string(),
            }
        );
        // Known extras map to Install (or AlreadyReady when installed).
        match dynamic::install_state("swift") {
            dynamic::InstallState::Installed => assert_eq!(
                syntax_confirm_for("swift"),
                SyntaxConfirm::AlreadyReady("Swift".to_string())
            ),
            _ => assert_eq!(
                syntax_confirm_for("swift"),
                SyntaxConfirm::Install {
                    id: "swift".to_string(),
                    pretty: "Swift".to_string(),
                }
            ),
        }
    }
}
