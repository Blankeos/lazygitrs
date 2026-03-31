mod config;
mod queries;

use std::collections::HashMap;
use std::path::Path;

use ratatui::prelude::*;
use tree_sitter_highlight::{HighlightEvent, Highlighter};

use crate::config::Theme;

use config::{LanguageConfig, CONFIGS, HIGHLIGHT_NAMES};

/// Map a highlight index to a ratatui Color using the active theme.
pub fn highlight_color(index: usize, theme: &Theme) -> Color {
    match HIGHLIGHT_NAMES.get(index) {
        Some(&"comment") => theme.syntax_comment,
        Some(&"keyword") => theme.syntax_keyword,
        Some(&"string" | &"string.special") => theme.syntax_string,
        Some(&"number" | &"constant" | &"constant.builtin") => theme.syntax_number,
        Some(&"function" | &"function.builtin" | &"function.method") => theme.syntax_function,
        Some(&"function.macro") => theme.syntax_function_macro,
        Some(&"type" | &"type.builtin" | &"constructor") => theme.syntax_type,
        Some(&"variable.builtin") => theme.syntax_variable_builtin,
        Some(&"variable.member" | &"property") => theme.syntax_variable_member,
        Some(&"module") => theme.syntax_module,
        Some(&"operator") => theme.syntax_operator,
        Some(&"tag") => theme.syntax_tag,
        Some(&"attribute") => theme.syntax_attribute,
        Some(&"label") => theme.syntax_label,
        Some(&"punctuation" | &"punctuation.bracket" | &"punctuation.delimiter") => {
            theme.syntax_punctuation
        }
        _ => theme.syntax_default,
    }
}

fn get_config_for_file(filename: &str) -> Option<&'static LanguageConfig> {
    let ext = Path::new(filename).extension().and_then(|e| e.to_str())?;
    CONFIGS.iter().find(|(e, _)| *e == ext).map(|(_, c)| c)
}

/// Pre-computed highlights for an entire file, organized by line number.
/// Handles multi-line constructs like JSDoc comments properly.
#[derive(Default)]
pub struct FileHighlighter {
    line_highlights: HashMap<usize, Vec<(String, Option<usize>)>>,
}

impl FileHighlighter {
    pub fn new(content: &str, filename: &str) -> Self {
        let Some(lang_config) = get_config_for_file(filename) else {
            return Self::default();
        };

        let mut highlighter = Highlighter::new();
        let highlights =
            highlighter.highlight(&lang_config.config, content.as_bytes(), None, |_| None);

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
    pub fn get_line_spans<'a>(&self, line_number: usize, bg: Option<Color>, theme: &Theme) -> Vec<Span<'a>> {
        let bg_color = bg.unwrap_or(Color::Reset);
        let default_fg = theme.syntax_default;

        self.line_highlights
            .get(&line_number)
            .map(|spans| {
                spans
                    .iter()
                    .filter(|(text, _)| *text != "\n")
                    .map(|(text, highlight_idx)| {
                        let fg = highlight_idx.map(|i| highlight_color(i, theme)).unwrap_or(default_fg);
                        Span::styled(text.clone(), Style::default().fg(fg).bg(bg_color))
                    })
                    .collect()
            })
            .unwrap_or_default()
    }
}
