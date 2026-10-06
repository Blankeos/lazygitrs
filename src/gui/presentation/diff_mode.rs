use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear, List, ListItem, ListState, Paragraph};

use crate::config::Theme;
use crate::gui::modes::diff_mode::{
    CompareDiffSource, CompareLayout, DiffModeFocus, DiffModeState, RefKind,
};
use crate::gui::presentation::commit_files::commit_file_status_display;
use crate::gui::presentation::commits::{CommitListCache, render_compare_commit_list_window};
use crate::gui::presentation::files::append_file_stats;
use crate::model::file_tree::CommitFileTreeNode;
use crate::pager::side_by_side::{self, DiffViewState};

/// Max items visible in the dropdown at once.
const DROPDOWN_MAX_VISIBLE: usize = 10;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Commit, CommitStatus, commit::Divergence};
    use ratatui::{Terminal, backend::TestBackend};

    fn commit(hash: &str, name: &str, divergence: Divergence) -> Commit {
        Commit {
            hash: hash.into(),
            name: name.into(),
            status: CommitStatus::Pushed,
            action: String::new(),
            tags: vec![],
            refs: vec![],
            extra_info: String::new(),
            author_name: "Test Author".into(),
            author_email: String::new(),
            unix_timestamp: 0,
            parents: vec![],
            divergence,
        }
    }

    fn buffer_text(terminal: &Terminal<TestBackend>) -> String {
        let buffer = terminal.backend().buffer();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer.cell((x, y)).unwrap().symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn compare_commits_panel_shows_sides_and_default_commit_rows() {
        let mut state = DiffModeState::new();
        state.ref_a = "main".into();
        state.ref_b = "feature".into();
        state.ahead_behind = Some((1, 1));
        state.commits = vec![
            commit("aaaaaaaa", "Main change", Divergence::Left),
            commit("bbbbbbbb", "Feature change", Divergence::Right),
        ];
        let mut cache = CommitListCache::default();
        let mut terminal = Terminal::new(TestBackend::new(60, 6)).unwrap();
        terminal
            .draw(|frame| {
                render_compare_commits(
                    frame,
                    frame.area(),
                    &mut state,
                    &Theme::default(),
                    &mut cache,
                )
            })
            .unwrap();
        let text = buffer_text(&terminal);
        assert!(text.contains("4 Commits · 1 ahead, 1 behind"), "{text}");
        assert!(text.contains("A aaaaaaaa"), "{text}");
        assert!(text.contains("B bbbbbbbb"), "{text}");
        assert!(text.contains("Main change"));
        assert!(text.contains("Feature change"));
        assert!(!text.contains("Enter: files"));
        assert!(!text.contains("A-only / B-only"));
        let bottom_border: String = (0..60)
            .map(|x| terminal.backend().buffer().cell((x, 5)).unwrap().symbol())
            .collect();
        assert_eq!(bottom_border, format!("└{}┘", "─".repeat(58)));
    }

    #[test]
    fn compare_help_bar_shows_commit_actions_without_duplicate_counts() {
        for (focus, counts, more) in [
            (DiffModeFocus::Commits, Some((3, 1)), true),
            (DiffModeFocus::Commits, Some((0, 0)), false),
            (DiffModeFocus::CommitFiles, Some((3, 1)), true),
            (DiffModeFocus::Commits, None, false),
        ] {
            let mut state = DiffModeState::new();
            state.set_focus(focus);
            state.ahead_behind = counts;
            let mut terminal = Terminal::new(TestBackend::new(80, 1)).unwrap();
            terminal
                .draw(|frame| {
                    render_status_bar(
                        frame,
                        frame.area(),
                        &state,
                        &DiffViewState::default(),
                        &Theme::default(),
                        &crate::config::KeybindingConfig::default(),
                    )
                })
                .unwrap();
            let text = buffer_text(&terminal);
            assert!(!text.contains("A vs B:"), "{text}");
            assert!(!text.contains("ahead"), "{text}");
            assert!(!text.contains("behind"), "{text}");
            assert_eq!(
                text.contains("Enter files"),
                focus == DiffModeFocus::Commits,
                "{text}"
            );
            assert_eq!(
                text.contains("PgDn more"),
                focus == DiffModeFocus::Commits && more,
                "{text}"
            );
        }
    }

    #[test]
    fn tree_compare_status_uses_configured_keys_and_hides_disabled_ones() {
        let mut state = DiffModeState::new();
        state.show_tree = true;
        let mut kb = crate::config::KeybindingConfig::default();
        kb.universal.fold_directory = "f".into();
        kb.universal.tree_parent = "p".into();
        kb.universal.tree_child = "c".into();
        kb.universal.tree_prev_sibling.clear();
        kb.universal.tree_next_sibling = "n".into();
        for focus in [
            DiffModeFocus::CommitFiles,
            DiffModeFocus::DiffExploration,
            DiffModeFocus::Commits,
        ] {
            state.set_focus(focus);
            let mut terminal = Terminal::new(TestBackend::new(180, 1)).unwrap();
            terminal
                .draw(|frame| {
                    render_status_bar(
                        frame,
                        frame.area(),
                        &state,
                        &DiffViewState::default(),
                        &Theme::default(),
                        &kb,
                    )
                })
                .unwrap();
            let text = buffer_text(&terminal);
            assert_eq!(
                text.contains("f fold"),
                focus == DiffModeFocus::CommitFiles,
                "{text}"
            );
            assert_eq!(
                text.contains("p/c nav"),
                focus != DiffModeFocus::Commits,
                "{text}"
            );
            assert_eq!(
                text.contains("n siblings"),
                focus != DiffModeFocus::Commits,
                "{text}"
            );
            assert!(!text.contains(",/."), "{text}");
            assert!(!text.contains("- fold"), "{text}");
        }
    }

    #[test]
    fn compare_empty_commit_states_are_distinct() {
        for (counts, expected) in [
            (Some((0, 0)), "Same commit"),
            (None, "Commit history unavailable"),
        ] {
            let mut state = DiffModeState::new();
            state.ref_a = "A".into();
            state.ref_b = "B".into();
            state.ahead_behind = counts;
            let mut terminal = Terminal::new(TestBackend::new(60, 5)).unwrap();
            terminal
                .draw(|frame| {
                    render_compare_commits(
                        frame,
                        frame.area(),
                        &mut state,
                        &Theme::default(),
                        &mut CommitListCache::default(),
                    )
                })
                .unwrap();
            assert!(buffer_text(&terminal).contains(expected));
        }
    }

    #[test]
    fn compare_layout_renders_four_sidebar_panels_and_handles_small_terminals() {
        use crate::gui::ScreenMode;
        for (width, height) in [(180, 30), (80, 40), (80, 14), (30, 8), (5, 3), (1, 1)] {
            for mode in [ScreenMode::Normal, ScreenMode::Half, ScreenMode::Full] {
                for focus in [
                    DiffModeFocus::SelectorA,
                    DiffModeFocus::SelectorB,
                    DiffModeFocus::CommitFiles,
                    DiffModeFocus::Commits,
                    DiffModeFocus::DiffExploration,
                ] {
                    let mut state = DiffModeState::new();
                    state.set_focus(focus);
                    let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                    terminal
                        .draw(|frame| {
                            render(
                                frame,
                                &mut state,
                                &mut DiffViewState::default(),
                                &Theme::default(),
                                false,
                                false,
                                &mut CommitListCache::default(),
                                1.0 / 3.0,
                                mode,
                                &crate::config::KeybindingConfig::default(),
                            )
                        })
                        .unwrap();
                    let text = buffer_text(&terminal);
                    if width == 180 {
                        for (panel_focus, title) in [
                            (DiffModeFocus::SelectorA, "1 A"),
                            (DiffModeFocus::SelectorB, "2 B"),
                            (DiffModeFocus::CommitFiles, "3 Files"),
                            (DiffModeFocus::Commits, "4 Commits"),
                            (DiffModeFocus::DiffExploration, "5 Diff"),
                        ] {
                            assert_eq!(
                                text.contains(title),
                                mode != ScreenMode::Full || focus == panel_focus,
                                "{mode:?} {focus:?}: {text}"
                            );
                        }
                    }
                    if width == 80 && height == 40 && mode != ScreenMode::Full {
                        for title in ["1 A", "2 B", "3 Files", "4 Commits", "5 Diff"] {
                            assert!(text.contains(title), "{mode:?} {focus:?}: {text}");
                        }
                    }
                }
            }
        }
    }
}

pub fn render(
    frame: &mut Frame,
    state: &mut DiffModeState,
    diff_view: &mut DiffViewState,
    theme: &Theme,
    diff_loading: bool,
    diff_loading_show: bool,
    commit_cache: &mut CommitListCache,
    side_ratio: f64,
    screen_mode: crate::gui::ScreenMode,
    keybindings: &crate::config::KeybindingConfig,
) {
    let layout = CompareLayout::new(frame.area(), side_ratio, screen_mode, state);
    let sidebar = layout.sidebar;

    if !sidebar[0].is_empty() {
        render_selector(frame, sidebar[0], state, DiffModeFocus::SelectorA, theme);
    }
    if !sidebar[1].is_empty() {
        render_selector(frame, sidebar[1], state, DiffModeFocus::SelectorB, theme);
    }
    if !sidebar[2].is_empty() {
        render_commit_files(frame, sidebar[2], state, theme);
    }
    if !sidebar[3].is_empty() {
        render_compare_commits(frame, sidebar[3], state, theme, commit_cache);
    }

    if !layout.diff.is_empty() {
        render_diff_panel(
            frame,
            layout.diff,
            state,
            diff_view,
            theme,
            diff_loading,
            diff_loading_show,
        );
        // Selection overlay must be before popups/dropdowns.
        crate::gui::views::render_selection_overlay(frame, diff_view, layout.diff, theme);
    }

    // Status bar
    render_status_bar(frame, layout.status, state, diff_view, theme, keybindings);

    // Render combobox dropdown overlay on top of the sidebar
    if state.editing.is_some() && sidebar.iter().any(|r| !r.is_empty()) {
        render_dropdown(frame, sidebar, state, theme);
    }
}

fn render_selector(
    frame: &mut Frame,
    area: Rect,
    state: &DiffModeState,
    which: DiffModeFocus,
    theme: &Theme,
) {
    let (focused, editing, display, number_label) = match which {
        DiffModeFocus::SelectorA => (
            state.focus == DiffModeFocus::SelectorA,
            matches!(
                state.editing,
                Some(crate::gui::modes::diff_mode::DiffModeSelector::A)
            ),
            &state.ref_a_display,
            " 1 A ",
        ),
        DiffModeFocus::SelectorB => (
            state.focus == DiffModeFocus::SelectorB,
            matches!(
                state.editing,
                Some(crate::gui::modes::diff_mode::DiffModeSelector::B)
            ),
            &state.ref_b_display,
            " 2 B ",
        ),
        _ => return,
    };

    let border = if focused || editing {
        theme.active_border
    } else {
        Style::default().fg(theme.text_dimmed)
    };
    let block = Block::default()
        .title(number_label)
        .borders(Borders::ALL)
        .border_style(border);
    if editing {
        // Render the textarea inside the block
        if let Some(ref ta) = state.textarea {
            let inner = block.inner(area);
            frame.render_widget(block, area);
            frame.render_widget(&*ta, inner);
        }
    } else {
        let text = if display.is_empty() {
            "Press Enter to select ref..."
        } else {
            display.as_str()
        };
        let style = if display.is_empty() {
            Style::default().fg(theme.text_dimmed)
        } else {
            Style::default().fg(theme.accent)
        };
        let widget = Paragraph::new(Span::styled(format!(" {}", text), style)).block(block);
        frame.render_widget(widget, area);
    }
}

fn render_compare_commits(
    frame: &mut Frame,
    area: Rect,
    state: &mut DiffModeState,
    theme: &Theme,
    cache: &mut CommitListCache,
) {
    let border = if state.focus == DiffModeFocus::Commits {
        theme.active_border
    } else {
        Style::default().fg(theme.text_dimmed)
    };
    let summary = state
        .ahead_behind
        .map(|(ahead, behind)| format!("{ahead} ahead, {behind} behind"));
    let title = summary
        .map(|summary| format!(" 4 Commits · {summary} "))
        .unwrap_or_else(|| " 4 Commits ".into());
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border);
    let inner = block.inner(area);
    if state.commits.is_empty() {
        let message = if !state.has_both_refs() {
            "Select refs A and B to compare"
        } else if state.ahead_behind == Some((0, 0)) {
            "Same commit — no unique commits"
        } else if state.ahead_behind.is_none() {
            "Commit history unavailable for these refs"
        } else {
            "No commits loaded"
        };
        frame.render_widget(
            Paragraph::new(Span::styled(
                format!(" {message}"),
                Style::default().fg(theme.text_dimmed),
            ))
            .block(block),
            area,
        );
        return;
    }
    let height = inner.height as usize;
    if height == 0 {
        frame.render_widget(block, area);
        return;
    }
    if !state.commits_viewport_manually_scrolled {
        crate::gui::scroll::ensure_visible(
            state.commits_selected,
            &mut state.commits_scroll,
            height,
        );
    }
    state.commits_scroll = state
        .commits_scroll
        .min(state.commits.len().saturating_sub(height));
    let items = render_compare_commit_list_window(
        &state.commits,
        state.commits_revision,
        theme,
        state.commits_scroll,
        height,
        cache,
    );
    let selection = state
        .commits_selected
        .checked_sub(state.commits_scroll)
        .filter(|&i| i < items.len());
    let mut list_state = ListState::default().with_selected(selection);
    frame.render_stateful_widget(
        List::new(items)
            .block(block)
            .highlight_style(theme.selected_line),
        area,
        &mut list_state,
    );
}

fn render_commit_files(frame: &mut Frame, area: Rect, state: &mut DiffModeState, theme: &Theme) {
    let focused = state.focus == DiffModeFocus::CommitFiles;
    let border = if focused {
        theme.active_border
    } else {
        Style::default().fg(theme.text_dimmed)
    };
    let tree_indicator = if state.show_tree { " (tree)" } else { "" };
    let title = format!(" 3 Files ({}{}) ", state.diff_files.len(), tree_indicator);
    let block = Block::default()
        .title(title)
        .borders(Borders::ALL)
        .border_style(border);
    let content_width = area.width.saturating_sub(2) as usize;
    let block = if let Some(hash) = &state.files_commit {
        block.title_bottom(format!(
            " {} · Esc: comparison ",
            &hash[..8.min(hash.len())]
        ))
    } else {
        block
    };

    if state.diff_files.is_empty() {
        let msg = if state.has_both_refs() {
            "No files changed"
        } else {
            "Select refs A and B to compare"
        };
        let widget = Paragraph::new(Span::styled(
            format!(" {}", msg),
            Style::default().fg(theme.text_dimmed),
        ))
        .block(block);
        frame.render_widget(widget, area);
        return;
    }

    // Build all items
    let items: Vec<ListItem> = if state.show_tree {
        state
            .tree_nodes
            .iter()
            .map(|node| render_tree_node(node, state, theme, content_width))
            .collect()
    } else {
        state
            .diff_files
            .iter()
            .map(|file| {
                let (status_style, status_icon) = commit_file_status_display(file, theme);
                let dim_style = Style::default().fg(theme.text_dimmed);
                let name_style = Style::default().fg(theme.text_strong);

                if file.rename_paths().is_some() {
                    let spans = vec![
                        Span::styled(format!(" {} ", status_icon), status_style),
                        Span::styled(file.name.clone(), name_style),
                    ];
                    return ListItem::new(Line::from(append_file_stats(
                        spans,
                        file.hunk_count,
                        file.additions,
                        file.deletions,
                        theme,
                        content_width,
                    )));
                }

                let path = file.name.as_str();
                let (dir, name) = match path.rfind('/') {
                    Some(idx) => (&path[..=idx], &path[idx + 1..]),
                    None => ("", path),
                };

                let mut spans = vec![
                    Span::styled(format!(" {} ", status_icon), status_style),
                    Span::styled(name.to_string(), name_style),
                ];
                if !dir.is_empty() {
                    spans.push(Span::styled(format!(" {}", dir), dim_style));
                }

                ListItem::new(Line::from(append_file_stats(
                    spans,
                    file.hunk_count,
                    file.additions,
                    file.deletions,
                    theme,
                    content_width,
                )))
            })
            .collect()
    };

    if items.is_empty() {
        frame.render_widget(block, area);
        return;
    }

    let inner = block.inner(area);
    let visible_height = inner.height as usize;
    if visible_height == 0 {
        frame.render_widget(block, area);
        return;
    }

    // Smart scroll: ensure selected is visible, only adjust when needed.
    // Skip when viewport was manually scrolled (mouse scroll) to avoid snapping back.
    if !state.viewport_manually_scrolled {
        crate::gui::scroll::ensure_visible(
            state.diff_files_selected,
            &mut state.diff_files_scroll,
            visible_height,
        );
    }
    let max_offset = items.len().saturating_sub(visible_height);
    if state.diff_files_scroll > max_offset {
        state.diff_files_scroll = max_offset;
    }
    let offset = state.diff_files_scroll;
    let selected = state.diff_files_selected;

    // Slice visible window and apply highlight to selected item
    let visible_items: Vec<ListItem> = items
        .into_iter()
        .skip(offset)
        .take(visible_height)
        .enumerate()
        .map(|(i, item)| {
            let idx = i + offset;
            if focused && idx == selected {
                item.style(theme.selected_line)
            } else {
                item
            }
        })
        .collect();

    let list = List::new(visible_items).block(block);
    frame.render_widget(list, area);
}

fn render_tree_node<'a>(
    node: &CommitFileTreeNode,
    state: &DiffModeState,
    theme: &Theme,
    width: usize,
) -> ListItem<'a> {
    let indent = "  ".repeat(node.depth);
    if node.is_dir {
        let is_collapsed = state.collapsed_dirs.contains(&node.path);
        let icon = if is_collapsed { "▶ " } else { "▼ " };
        let is_root = node.path == ".";
        let dir_style = Style::default().fg(theme.text_dimmed);
        let line = if is_root {
            Line::from(Span::styled(format!("  {} /", icon.trim_end()), dir_style))
        } else {
            Line::from(vec![
                Span::styled(format!("  {}{}", indent, icon), dir_style),
                Span::styled(node.name.clone(), dir_style),
            ])
        };
        ListItem::new(line)
    } else if let Some(file_idx) = node.file_index {
        if let Some(file) = state.diff_files.get(file_idx) {
            let (status_style, status_icon) = commit_file_status_display(file, theme);
            let spans = vec![
                Span::raw(format!("  {}", indent)),
                Span::styled(format!("{} ", status_icon), status_style),
                Span::styled(node.name.clone(), Style::default().fg(theme.text_strong)),
            ];
            let line = Line::from(append_file_stats(
                spans,
                file.hunk_count,
                file.additions,
                file.deletions,
                theme,
                width,
            ));
            ListItem::new(line)
        } else {
            ListItem::new(Line::raw(""))
        }
    } else {
        ListItem::new(Line::raw(""))
    }
}

fn render_diff_panel(
    frame: &mut Frame,
    area: Rect,
    state: &DiffModeState,
    diff_view: &mut DiffViewState,
    theme: &Theme,
    diff_loading: bool,
    diff_loading_show: bool,
) {
    let focused = state.focus == DiffModeFocus::DiffExploration;

    if !diff_view.is_empty() {
        side_by_side::render_diff(frame, area, diff_view, theme, focused, diff_loading, false);
        side_by_side::render_diff_search_highlights(frame, area, diff_view, theme);
        side_by_side::render_diff_search_bar(frame, area, diff_view, theme);
    } else {
        let border = if focused {
            theme.active_border
        } else {
            Style::default().fg(theme.text_dimmed)
        };
        let block = Block::default()
            .title(" 5 Diff ")
            .borders(Borders::ALL)
            .border_style(border);
        let msg = if diff_loading_show {
            " Loading diff..."
        } else if state.diff_source == CompareDiffSource::Commit {
            " No patch for this commit"
        } else if !state.has_both_refs() || state.diff_files.is_empty() {
            " Select a file or commit to view diff"
        } else {
            ""
        };
        let widget =
            Paragraph::new(Span::styled(msg, Style::default().fg(theme.text_dimmed))).block(block);
        frame.render_widget(widget, area);
    }
}

fn render_status_bar(
    frame: &mut Frame,
    area: Rect,
    state: &DiffModeState,
    diff_view: &DiffViewState,
    theme: &Theme,
    keybindings: &crate::config::KeybindingConfig,
) {
    let tree_hints =
        crate::gui::controller::tree::hints(keybindings, state.focus == DiffModeFocus::CommitFiles);
    // If search is active or has results, show search bar instead of hints
    if state.file_search_active {
        if let Some(ref ta) = state.file_search_textarea {
            let match_info = if !state.file_search_matches.is_empty() {
                format!(
                    " {}/{}",
                    state.file_search_match_idx + 1,
                    state.file_search_matches.len()
                )
            } else if !state.file_search_query.is_empty() {
                " (no matches)".to_string()
            } else {
                String::new()
            };

            let prefix_width = 2u16; // " /"
            let suffix_width = match_info.len() as u16;
            let ta_width = area.width.saturating_sub(prefix_width + suffix_width);

            let prefix_rect = Rect::new(area.x, area.y, prefix_width, 1);
            let prefix = Paragraph::new(Span::styled(
                " /",
                Style::default().fg(theme.accent_secondary),
            ));
            frame.render_widget(prefix, prefix_rect);

            let ta_rect = Rect::new(area.x + prefix_width, area.y, ta_width, 1);
            frame.render_widget(&*ta, ta_rect);

            if !match_info.is_empty() {
                let suffix_rect =
                    Rect::new(area.x + prefix_width + ta_width, area.y, suffix_width, 1);
                let suffix = Paragraph::new(Span::styled(
                    match_info,
                    Style::default().fg(theme.accent_secondary),
                ));
                frame.render_widget(suffix, suffix_rect);
            }
            return;
        }
    } else if !state.file_search_query.is_empty() {
        // Search dismissed but results persist — show query + match info
        let match_info = if !state.file_search_matches.is_empty() {
            format!(
                " {}/{}",
                state.file_search_match_idx + 1,
                state.file_search_matches.len()
            )
        } else {
            " (no matches)".to_string()
        };
        let bar = Paragraph::new(Span::styled(
            format!(" /{}{}", state.file_search_query, match_info),
            Style::default().fg(theme.accent_secondary),
        ));
        frame.render_widget(bar, area);
        return;
    }

    let mut hints = if state.editing.is_some() {
        vec![("Enter", "select"), ("Esc", "cancel"), ("↑↓", "navigate")]
    } else {
        let view_layout_hint = match diff_view.view_layout {
            side_by_side::DiffViewLayout::SideBySide => "unified view",
            side_by_side::DiffViewLayout::Unified => "split view",
        };
        let mut hints = Vec::new();
        if state.focus == DiffModeFocus::Commits {
            hints.push(("Enter", "files"));
            if state.has_more_commits() {
                hints.push(("PgDn", "more"));
            }
        }
        hints.extend([
            ("q", "exit"),
            ("Tab", "cycle"),
            ("1-5", "panel"),
            ("<c-s>", "swap"),
            ("`", "tree"),
        ]);
        if state.show_tree && state.focus == DiffModeFocus::CommitFiles {
            hints.extend(tree_hints.iter().map(|(key, label)| (key.as_str(), *label)));
        }
        hints.push(("\\", view_layout_hint));
        hints.push(("?", "help"));
        hints
    };

    if state.show_tree && state.focus == DiffModeFocus::DiffExploration {
        hints.extend(tree_hints.iter().map(|(key, label)| (key.as_str(), *label)));
    }

    let key_style = Style::default().fg(theme.text).add_modifier(Modifier::BOLD);
    let desc_style = Style::default().fg(theme.text_dimmed);
    let spans: Vec<Span> = hints
        .iter()
        .flat_map(|(key, desc)| {
            vec![
                Span::styled(format!(" {} ", key), key_style),
                Span::styled(format!("{} ", desc), desc_style),
            ]
        })
        .collect();

    let bar = Paragraph::new(Line::from(spans));
    frame.render_widget(bar, area);
}

fn render_dropdown(frame: &mut Frame, sidebar: [Rect; 4], state: &DiffModeState, theme: &Theme) {
    // Position dropdown below the relevant selector
    let anchor = if matches!(
        state.editing,
        Some(crate::gui::modes::diff_mode::DiffModeSelector::A)
    ) {
        sidebar[0]
    } else {
        sidebar[1]
    };

    let total = state.search_results.len();
    if total == 0 {
        return;
    }

    let max_items = DROPDOWN_MAX_VISIBLE.min(total);
    let dropdown_height = (max_items as u16) + 2; // +2 for borders
    let available_height = frame.area().height.saturating_sub(anchor.y + anchor.height);
    let dropdown_area = Rect {
        x: anchor.x,
        y: anchor.y + anchor.height,
        width: anchor.width,
        height: dropdown_height.min(available_height),
    };

    if dropdown_area.height < 3 {
        return;
    }

    frame.render_widget(Clear, dropdown_area);

    // Compute visible window
    let visible_count = (dropdown_area.height as usize).saturating_sub(2); // -2 for borders
    let scroll = state.dropdown_scroll;
    let visible_end = (scroll + visible_count).min(total);

    let items: Vec<ListItem> = state
        .search_results
        .iter()
        .skip(scroll)
        .take(visible_end - scroll)
        .map(|candidate| {
            let kind_label = match candidate.kind {
                RefKind::RawRef => Span::styled("[ref] ", Style::default().fg(theme.text_strong)),
                RefKind::Branch => Span::styled("[branch] ", Style::default().fg(theme.ref_local)),
                RefKind::RemoteBranch => {
                    Span::styled("[remote] ", Style::default().fg(theme.ref_remote))
                }
                RefKind::Tag => Span::styled("[tag] ", Style::default().fg(theme.ref_tag)),
                RefKind::Commit => {
                    Span::styled("[commit] ", Style::default().fg(theme.reflog_hash))
                }
            };
            let line = Line::from(vec![
                Span::raw(" "),
                kind_label,
                Span::styled(
                    candidate.display.clone(),
                    Style::default().fg(theme.text_strong),
                ),
            ]);
            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(theme.active_border);

    let list = List::new(items)
        .block(block)
        .highlight_style(theme.selected_line);

    let mut list_state = ListState::default();
    // Selected index relative to the visible window
    let relative_selected = state.search_selected.saturating_sub(scroll);
    list_state.select(Some(relative_selected));
    frame.render_stateful_widget(list, dropdown_area, &mut list_state);
}
