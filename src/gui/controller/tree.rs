//! Shared tree shortcuts for sidebar and diff-focused navigation.
use crate::config::KeybindingConfig;
pub use crate::config::matches_key;
use crate::model::file_tree::{self, NavigableTreeNode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

pub fn destination<T: NavigableTreeNode>(
    key: KeyEvent,
    kb: &KeybindingConfig,
    nodes: &[T],
    selected: usize,
) -> Option<Option<usize>> {
    let u = &kb.universal;
    if matches_key(key, &u.tree_parent) {
        Some(file_tree::find_parent_idx(nodes, selected))
    } else if matches_key(key, &u.tree_child) {
        Some(file_tree::find_first_child_idx(nodes, selected))
    } else if matches_key(key, &u.tree_prev_sibling) {
        Some(file_tree::find_prev_sibling_idx(nodes, selected))
    } else if matches_key(key, &u.tree_next_sibling) {
        Some(file_tree::find_next_sibling_idx(nodes, selected))
    } else {
        None
    }
}

pub fn command_section(
    kb: &KeybindingConfig,
    include_fold: bool,
) -> crate::gui::popup::CommandSection {
    use crate::gui::popup::{CommandEntry, CommandSection};
    let u = &kb.universal;
    let mut entries = Vec::new();
    if include_fold && !u.fold_directory.is_empty() {
        entries.push(CommandEntry::keybinding(
            u.fold_directory.clone(),
            "Fold / unfold directory".into(),
        ));
    }
    for (key, label) in [
        (&u.tree_parent, "Select parent"),
        (&u.tree_child, "Select first visible child"),
        (&u.tree_prev_sibling, "Select previous sibling"),
        (&u.tree_next_sibling, "Select next sibling"),
    ] {
        if !key.is_empty() {
            entries.push(CommandEntry::keybinding(key.clone(), label.into()));
        }
    }
    CommandSection {
        title: "File Tree".into(),
        entries,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_help_reflects_custom_and_disabled_bindings() {
        let mut kb = KeybindingConfig::default();
        kb.universal.fold_directory = "<c-f>".into();
        kb.universal.tree_parent = "p".into();
        kb.universal.tree_child = "c".into();
        kb.universal.tree_prev_sibling.clear();
        kb.universal.tree_next_sibling = "n".into();
        for include_fold in [false, true] {
            let section = command_section(&kb, include_fold);
            assert_eq!(section.title, "File Tree");
            let keys: Vec<&str> = section.entries.iter().map(|e| e.key.as_str()).collect();
            assert_eq!(
                keys,
                if include_fold {
                    vec!["<c-f>", "p", "c", "n"]
                } else {
                    vec!["p", "c", "n"]
                }
            );
        }
        kb.universal.fold_directory.clear();
        kb.universal.tree_parent.clear();
        kb.universal.tree_child.clear();
        kb.universal.tree_next_sibling.clear();
        assert!(command_section(&kb, true).entries.is_empty());
    }

    #[test]
    fn tree_punctuation_matches_shift_but_rejects_extra_modifiers() {
        assert!(matches_key(
            KeyEvent::new(KeyCode::Char('>'), KeyModifiers::SHIFT),
            ">"
        ));
        assert!(matches_key(
            KeyEvent::new(KeyCode::Char('>'), KeyModifiers::NONE),
            ">"
        ));
        assert!(!matches_key(
            KeyEvent::new(
                KeyCode::Char('>'),
                KeyModifiers::SHIFT | KeyModifiers::CONTROL
            ),
            ">"
        ));
        assert!(!matches_key(
            KeyEvent::new(KeyCode::Char('p'), KeyModifiers::SHIFT),
            "p"
        ));
        assert!(!matches_key(
            KeyEvent::new(KeyCode::Char('.'), KeyModifiers::NONE),
            ""
        ));
    }
}
