use anyhow::Result;
use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};

use crate::git::rebase::RebaseAction;
use crate::gui::Gui;
use crate::gui::popup::{HelpEntry, HelpSection, MessageKind, PopupState};

pub fn handle_key(gui: &mut Gui, key: KeyEvent) -> Result<()> {
    // Popup takes priority
    if gui.popup != PopupState::None {
        return gui.handle_popup_key(key);
    }

    // q or Esc: abort / exit without rebasing
    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
        gui.rebase_mode.exit();
        return Ok(());
    }

    // ? to show help
    if key.code == KeyCode::Char('?') {
        show_rebase_help(gui);
        return Ok(());
    }

    // Enter: execute the rebase
    if key.code == KeyCode::Enter {
        return execute_rebase(gui);
    }

    let entry_count = gui.rebase_mode.entries.len();
    if entry_count == 0 {
        return Ok(());
    }

    // Navigation: j/Down to move selection down, k/Up to move up
    match key.code {
        KeyCode::Char('j') | KeyCode::Down if !key.modifiers.contains(KeyModifiers::ALT) => {
            if gui.rebase_mode.selected + 1 < entry_count {
                gui.rebase_mode.selected += 1;
            }
            return Ok(());
        }
        KeyCode::Char('k') | KeyCode::Up if !key.modifiers.contains(KeyModifiers::ALT) => {
            if gui.rebase_mode.selected > 0 {
                gui.rebase_mode.selected -= 1;
            }
            return Ok(());
        }
        _ => {}
    }

    // Action shortcuts
    match key.code {
        KeyCode::Char('p') => {
            gui.rebase_mode.set_action(RebaseAction::Pick);
            return Ok(());
        }
        KeyCode::Char('r') => {
            gui.rebase_mode.set_action(RebaseAction::Reword);
            return Ok(());
        }
        KeyCode::Char('e') => {
            gui.rebase_mode.set_action(RebaseAction::Edit);
            return Ok(());
        }
        KeyCode::Char('s') => {
            gui.rebase_mode.set_action(RebaseAction::Squash);
            return Ok(());
        }
        KeyCode::Char('f') => {
            gui.rebase_mode.set_action(RebaseAction::Fixup);
            return Ok(());
        }
        KeyCode::Char('d') => {
            gui.rebase_mode.set_action(RebaseAction::Drop);
            return Ok(());
        }
        _ => {}
    }

    // h/Left: cycle action backward, l/Right: cycle action forward
    match key.code {
        KeyCode::Char('l') | KeyCode::Right => {
            gui.rebase_mode.cycle_action_forward();
            return Ok(());
        }
        KeyCode::Char('h') | KeyCode::Left => {
            gui.rebase_mode.cycle_action_backward();
            return Ok(());
        }
        _ => {}
    }

    // Alt+Up / Alt+k: move entry up
    if (key.code == KeyCode::Up || key.code == KeyCode::Char('k'))
        && key.modifiers.contains(KeyModifiers::ALT)
    {
        gui.rebase_mode.move_up();
        return Ok(());
    }

    // Alt+Down / Alt+j: move entry down
    if (key.code == KeyCode::Down || key.code == KeyCode::Char('j'))
        && key.modifiers.contains(KeyModifiers::ALT)
    {
        gui.rebase_mode.move_down();
        return Ok(());
    }

    // [ : swap selected entry with previous (move action up, keep selection)
    if key.code == KeyCode::Char('[') {
        gui.rebase_mode.move_up();
        return Ok(());
    }

    // ] : swap selected entry with next (move action down, keep selection)
    if key.code == KeyCode::Char(']') {
        gui.rebase_mode.move_down();
        return Ok(());
    }

    // g: jump to top, G: jump to bottom
    if key.code == KeyCode::Char('g') {
        gui.rebase_mode.selected = 0;
        return Ok(());
    }
    if key.code == KeyCode::Char('G') {
        gui.rebase_mode.selected = entry_count.saturating_sub(1);
        return Ok(());
    }

    Ok(())
}

fn execute_rebase(gui: &mut Gui) -> Result<()> {
    let actions = gui.rebase_mode.build_actions();
    let base_hash = gui.rebase_mode.base_hash.clone();

    // Validate: squash/fixup cannot be the first action
    if let Some((_, first_action)) = actions.first() {
        if *first_action == RebaseAction::Squash || *first_action == RebaseAction::Fixup {
            gui.popup = PopupState::Message {
                title: "Invalid rebase".to_string(),
                message: format!(
                    "Cannot {} the first commit — there is nothing to {} into.",
                    first_action.as_str(),
                    first_action.as_str(),
                ),
                kind: MessageKind::Error,
            };
            return Ok(());
        }
    }

    match gui.git.rebase_interactive_batch(&base_hash, &actions) {
        Ok(()) => {
            gui.rebase_mode.exit();
            gui.needs_refresh = true;
        }
        Err(e) => {
            gui.popup = PopupState::Message {
                title: "Rebase failed".to_string(),
                message: format!(
                    "{}\n\nThe repo may be in a rebase-in-progress state.\nUse 'git rebase --abort' to cancel or resolve conflicts externally.",
                    e
                ),
                kind: MessageKind::Error,
            };
        }
    }

    Ok(())
}

fn show_rebase_help(gui: &mut Gui) {
    let actions_section = HelpSection {
        title: "Actions".into(),
        entries: vec![
            HelpEntry { key: "p".into(), description: "Set action to Pick".into() },
            HelpEntry { key: "r".into(), description: "Set action to Reword".into() },
            HelpEntry { key: "e".into(), description: "Set action to Edit".into() },
            HelpEntry { key: "s".into(), description: "Set action to Squash".into() },
            HelpEntry { key: "f".into(), description: "Set action to Fixup".into() },
            HelpEntry { key: "d".into(), description: "Set action to Drop".into() },
            HelpEntry { key: "h / ←".into(), description: "Cycle action backward".into() },
            HelpEntry { key: "l / →".into(), description: "Cycle action forward".into() },
        ],
    };

    let navigation_section = HelpSection {
        title: "Navigation".into(),
        entries: vec![
            HelpEntry { key: "j / ↓".into(), description: "Select next commit".into() },
            HelpEntry { key: "k / ↑".into(), description: "Select previous commit".into() },
            HelpEntry { key: "g".into(), description: "Jump to top".into() },
            HelpEntry { key: "G".into(), description: "Jump to bottom".into() },
            HelpEntry { key: "Alt+↑".into(), description: "Move commit up".into() },
            HelpEntry { key: "Alt+↓".into(), description: "Move commit down".into() },
            HelpEntry { key: "[".into(), description: "Swap with previous".into() },
            HelpEntry { key: "]".into(), description: "Swap with next".into() },
        ],
    };

    let general_section = HelpSection {
        title: "General".into(),
        entries: vec![
            HelpEntry { key: "Enter".into(), description: "Start rebase".into() },
            HelpEntry { key: "q / Esc".into(), description: "Abort (exit without rebasing)".into() },
        ],
    };

    gui.popup = PopupState::Help {
        sections: vec![actions_section, navigation_section, general_section],
        selected: 0,
        search_textarea: crate::gui::popup::make_help_search_textarea(),
        scroll_offset: 0,
    };
}
