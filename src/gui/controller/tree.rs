//! Shared tree shortcuts for sidebar and diff-focused navigation.
use crate::config::KeybindingConfig;
use crate::config::keybindings::parse_key;
use crate::model::file_tree::{self, NavigableTreeNode};
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

/// Outer Option means the key is claimed; inner Option means a destination exists.
/// Boundary keys are consumed too, so they cannot fall through to unrelated actions.
pub fn matches_key(mut key: KeyEvent, binding: &str) -> bool {
    let Some(expected) = parse_key(binding) else {
        return false;
    };
    // Shifted punctuation may arrive with SHIFT on real terminals.
    // Do not strip CTRL/ALT or SHIFT for letters / explicit modifier bindings.
    if expected.modifiers.is_empty()
        && matches!(expected.code, KeyCode::Char(c) if c.is_ascii_punctuation())
    {
        key.modifiers.remove(KeyModifiers::SHIFT);
    }
    key.code == expected.code && key.modifiers == expected.modifiers
}

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

pub fn hints(kb: &KeybindingConfig, include_fold: bool) -> Vec<(String, &'static str)> {
    let u = &kb.universal;
    let mut hints = Vec::new();
    if include_fold && !u.fold_directory.is_empty() {
        hints.push((u.fold_directory.clone(), "fold"));
    }
    for (keys, label) in [
        ([&u.tree_parent, &u.tree_child], "nav"),
        ([&u.tree_prev_sibling, &u.tree_next_sibling], "siblings"),
    ] {
        let keys = keys
            .into_iter()
            .filter(|k| !k.is_empty())
            .cloned()
            .collect::<Vec<_>>()
            .join("/");
        if !keys.is_empty() {
            hints.push((keys, label));
        }
    }
    hints
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
    fn tree_hints_reflect_custom_and_disabled_bindings() {
        let mut kb = KeybindingConfig::default();
        kb.universal.fold_directory = "<c-f>".into();
        kb.universal.tree_parent = "p".into();
        kb.universal.tree_child = "c".into();
        kb.universal.tree_prev_sibling.clear();
        kb.universal.tree_next_sibling = "n".into();
        assert_eq!(
            hints(&kb, true),
            vec![
                ("<c-f>".into(), "fold"),
                ("p/c".into(), "nav"),
                ("n".into(), "siblings")
            ]
        );
        assert_eq!(
            hints(&kb, false),
            vec![("p/c".into(), "nav"), ("n".into(), "siblings")]
        );
        kb.universal.fold_directory.clear();
        kb.universal.tree_parent.clear();
        kb.universal.tree_child.clear();
        kb.universal.tree_next_sibling.clear();
        assert!(hints(&kb, true).is_empty());
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
