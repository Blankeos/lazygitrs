use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct KeybindingConfig {
    pub universal: UniversalKeybinding,
    pub status: StatusKeybinding,
    pub files: FilesKeybinding,
    pub branches: BranchesKeybinding,
    pub commits: CommitsKeybinding,
    pub stash: StashKeybinding,
    #[serde(rename = "commitMessage")]
    pub commit_message: CommitMessageKeybinding,
}

impl Default for KeybindingConfig {
    fn default() -> Self {
        Self {
            universal: UniversalKeybinding::default(),
            status: StatusKeybinding::default(),
            files: FilesKeybinding::default(),
            branches: BranchesKeybinding::default(),
            commits: CommitsKeybinding::default(),
            stash: StashKeybinding::default(),
            commit_message: CommitMessageKeybinding::default(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UniversalKeybinding {
    pub quit: String,
    #[serde(rename = "quit-alt1")]
    pub quit_alt1: String,
    #[serde(rename = "return")]
    pub return_key: String,
    #[serde(rename = "quitWithoutChangingDirectory")]
    pub quit_without_changing_directory: String,
    #[serde(rename = "togglePanel")]
    pub toggle_panel: String,
    #[serde(rename = "prevItem")]
    pub prev_item: String,
    #[serde(rename = "nextItem")]
    pub next_item: String,
    #[serde(rename = "prevItem-alt")]
    pub prev_item_alt: String,
    #[serde(rename = "nextItem-alt")]
    pub next_item_alt: String,
    #[serde(rename = "prevPage")]
    pub prev_page: String,
    #[serde(rename = "nextPage")]
    pub next_page: String,
    #[serde(rename = "scrollLeft")]
    pub scroll_left: String,
    #[serde(rename = "scrollRight")]
    pub scroll_right: String,
    #[serde(rename = "gotoTop")]
    pub goto_top: String,
    #[serde(rename = "gotoBottom")]
    pub goto_bottom: String,
    #[serde(rename = "prevBlock")]
    pub prev_block: String,
    #[serde(rename = "nextBlock")]
    pub next_block: String,
    #[serde(rename = "prevBlock-alt")]
    pub prev_block_alt: String,
    #[serde(rename = "nextBlock-alt")]
    pub next_block_alt: String,
    #[serde(rename = "nextMatch")]
    pub next_match: String,
    #[serde(rename = "prevMatch")]
    pub prev_match: String,
    #[serde(rename = "startSearch")]
    pub start_search: String,
    #[serde(rename = "optionMenu")]
    pub option_menu: String,
    pub edit: String,
    #[serde(rename = "openFile")]
    pub open_file: String,
    #[serde(rename = "scrollUpMain")]
    pub scroll_up_main: String,
    #[serde(rename = "scrollDownMain")]
    pub scroll_down_main: String,
    #[serde(rename = "scrollUpMain-alt1")]
    pub scroll_up_main_alt1: String,
    #[serde(rename = "scrollDownMain-alt1")]
    pub scroll_down_main_alt1: String,
    pub undo: String,
    pub redo: String,
    #[serde(rename = "filteringMenu")]
    pub filtering_menu: String,
    #[serde(rename = "diffingMenu")]
    pub diffing_menu: String,
    #[serde(rename = "copyToClipboard")]
    pub copy_to_clipboard: String,
    pub refresh: String,
    #[serde(rename = "createRebaseOptionsMenu")]
    pub create_rebase_options_menu: String,
    #[serde(rename = "pushFiles")]
    pub push_files: String,
    #[serde(rename = "pullFiles")]
    pub pull_files: String,
    #[serde(rename = "nextScreenMode")]
    pub next_screen_mode: String,
    #[serde(rename = "prevScreenMode")]
    pub prev_screen_mode: String,
    #[serde(rename = "createPatchOptionsMenu")]
    pub create_patch_options_menu: String,
}

impl Default for UniversalKeybinding {
    fn default() -> Self {
        Self {
            quit: "q".into(),
            quit_alt1: "<c-c>".into(),
            return_key: "<escape>".into(),
            quit_without_changing_directory: "Q".into(),
            toggle_panel: "<tab>".into(),
            prev_item: "k".into(),
            next_item: "j".into(),
            prev_item_alt: "<up>".into(),
            next_item_alt: "<down>".into(),
            prev_page: "<pgup>".into(),
            next_page: "<pgdown>".into(),
            scroll_left: "H".into(),
            scroll_right: "L".into(),
            goto_top: "<".into(),
            goto_bottom: ">".into(),
            prev_block: "<left>".into(),
            next_block: "<right>".into(),
            prev_block_alt: "h".into(),
            next_block_alt: "l".into(),
            next_match: "n".into(),
            prev_match: "N".into(),
            start_search: "/".into(),
            option_menu: "x".into(),
            edit: "e".into(),
            open_file: "o".into(),
            scroll_up_main: "<pgup>".into(),
            scroll_down_main: "<pgdown>".into(),
            scroll_up_main_alt1: "K".into(),
            scroll_down_main_alt1: "J".into(),
            undo: "z".into(),
            redo: "<c-z>".into(),
            filtering_menu: "<c-s>".into(),
            diffing_menu: "W".into(),
            copy_to_clipboard: "<c-o>".into(),
            refresh: "R".into(),
            create_rebase_options_menu: "m".into(),
            push_files: "P".into(),
            pull_files: "p".into(),
            next_screen_mode: "+".into(),
            prev_screen_mode: "_".into(),
            create_patch_options_menu: "<c-p>".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StatusKeybinding {
    #[serde(rename = "checkForUpdate")]
    pub check_for_update: String,
    #[serde(rename = "recentRepos")]
    pub recent_repos: String,
    #[serde(rename = "allBranchesLogGraph")]
    pub all_branches_log_graph: String,
}

impl Default for StatusKeybinding {
    fn default() -> Self {
        Self {
            check_for_update: "u".into(),
            recent_repos: "<enter>".into(),
            all_branches_log_graph: "a".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct FilesKeybinding {
    #[serde(rename = "commitChanges")]
    pub commit_changes: String,
    #[serde(rename = "commitChangesWithoutHook")]
    pub commit_changes_without_hook: String,
    #[serde(rename = "amendLastCommit")]
    pub amend_last_commit: String,
    #[serde(rename = "commitChangesWithEditor")]
    pub commit_changes_with_editor: String,
    #[serde(rename = "toggleStagedAll")]
    pub toggle_staged_all: String,
    #[serde(rename = "stashAllChanges")]
    pub stash_all_changes: String,
    #[serde(rename = "toggleTreeView")]
    pub toggle_tree_view: String,
    pub fetch: String,
    #[serde(rename = "ignoreFile")]
    pub ignore_file: String,
}

impl Default for FilesKeybinding {
    fn default() -> Self {
        Self {
            commit_changes: "c".into(),
            commit_changes_without_hook: "w".into(),
            amend_last_commit: "A".into(),
            commit_changes_with_editor: "C".into(),
            toggle_staged_all: "a".into(),
            stash_all_changes: "s".into(),
            toggle_tree_view: "`".into(),
            fetch: "f".into(),
            ignore_file: "i".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct BranchesKeybinding {
    #[serde(rename = "createPullRequest")]
    pub create_pull_request: String,
    #[serde(rename = "viewPullRequestOptions")]
    pub view_pull_request_options: String,
    #[serde(rename = "checkoutBranchByName")]
    pub checkout_branch_by_name: String,
    #[serde(rename = "forceCheckoutBranch")]
    pub force_checkout_branch: String,
    #[serde(rename = "rebaseBranch")]
    pub rebase_branch: String,
    #[serde(rename = "renameBranch")]
    pub rename_branch: String,
    #[serde(rename = "mergeIntoCurrentBranch")]
    pub merge_into_current_branch: String,
    #[serde(rename = "fastForward")]
    pub fast_forward: String,
    #[serde(rename = "setUpstream")]
    pub set_upstream: String,
}

impl Default for BranchesKeybinding {
    fn default() -> Self {
        Self {
            create_pull_request: "o".into(),
            view_pull_request_options: "O".into(),
            checkout_branch_by_name: "c".into(),
            force_checkout_branch: "F".into(),
            rebase_branch: "r".into(),
            rename_branch: "R".into(),
            merge_into_current_branch: "M".into(),
            fast_forward: "f".into(),
            set_upstream: "u".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommitsKeybinding {
    #[serde(rename = "squashDown")]
    pub squash_down: String,
    #[serde(rename = "renameCommit")]
    pub rename_commit: String,
    #[serde(rename = "renameCommitWithEditor")]
    pub rename_commit_with_editor: String,
    #[serde(rename = "viewResetOptions")]
    pub view_reset_options: String,
    #[serde(rename = "markCommitAsFixup")]
    pub mark_commit_as_fixup: String,
    #[serde(rename = "createFixupCommit")]
    pub create_fixup_commit: String,
    #[serde(rename = "squashAboveCommits")]
    pub squash_above_commits: String,
    #[serde(rename = "moveDownCommit")]
    pub move_down_commit: String,
    #[serde(rename = "moveUpCommit")]
    pub move_up_commit: String,
    #[serde(rename = "amendToCommit")]
    pub amend_to_commit: String,
    #[serde(rename = "pickCommit")]
    pub pick_commit: String,
    #[serde(rename = "revertCommit")]
    pub revert_commit: String,
    #[serde(rename = "cherryPickCopy")]
    pub cherry_pick_copy: String,
    #[serde(rename = "pasteCommits")]
    pub paste_commits: String,
    #[serde(rename = "tagCommit")]
    pub tag_commit: String,
    #[serde(rename = "checkoutCommit")]
    pub checkout_commit: String,
    #[serde(rename = "resetCherryPick")]
    pub reset_cherry_pick: String,
    #[serde(rename = "openLogMenu")]
    pub open_log_menu: String,
    #[serde(rename = "viewBisectOptions")]
    pub view_bisect_options: String,
}

impl Default for CommitsKeybinding {
    fn default() -> Self {
        Self {
            squash_down: "s".into(),
            rename_commit: "r".into(),
            rename_commit_with_editor: "R".into(),
            view_reset_options: "g".into(),
            mark_commit_as_fixup: "f".into(),
            create_fixup_commit: "F".into(),
            squash_above_commits: "S".into(),
            move_down_commit: "<c-j>".into(),
            move_up_commit: "<c-k>".into(),
            amend_to_commit: "A".into(),
            pick_commit: "p".into(),
            revert_commit: "t".into(),
            cherry_pick_copy: "C".into(),
            paste_commits: "V".into(),
            tag_commit: "T".into(),
            checkout_commit: "<space>".into(),
            reset_cherry_pick: "<c-q>".into(),
            open_log_menu: "<c-l>".into(),
            view_bisect_options: "b".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct StashKeybinding {
    #[serde(rename = "popStash")]
    pub pop_stash: String,
    #[serde(rename = "renameStash")]
    pub rename_stash: String,
}

impl Default for StashKeybinding {
    fn default() -> Self {
        Self {
            pop_stash: "g".into(),
            rename_stash: "r".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CommitMessageKeybinding {
    #[serde(rename = "commitMenu")]
    pub commit_menu: String,
    #[serde(rename = "aiGenerate")]
    pub ai_generate: String,
}

impl Default for CommitMessageKeybinding {
    fn default() -> Self {
        Self {
            commit_menu: "<c-o>".into(),
            ai_generate: "<c-g>".into(),
        }
    }
}

/// Parse a keybinding string like "q", "<c-c>", "<enter>", "<space>" into a KeyEvent.
pub fn parse_key(s: &str) -> Option<KeyEvent> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    // Check for modifier+key combos like <c-c>, <a-x>
    if s.starts_with('<') && s.ends_with('>') {
        let inner = &s[1..s.len() - 1];

        // Ctrl modifier
        if let Some(key) = inner.strip_prefix("c-") {
            let ch = key.chars().next()?;
            return Some(KeyEvent::new(
                KeyCode::Char(ch),
                KeyModifiers::CONTROL,
            ));
        }

        // Alt modifier
        if let Some(key) = inner.strip_prefix("a-") {
            let ch = key.chars().next()?;
            return Some(KeyEvent::new(KeyCode::Char(ch), KeyModifiers::ALT));
        }

        // Special keys
        return match inner {
            "enter" => Some(KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE)),
            "escape" | "esc" => Some(KeyEvent::new(KeyCode::Esc, KeyModifiers::NONE)),
            "tab" => Some(KeyEvent::new(KeyCode::Tab, KeyModifiers::NONE)),
            "backspace" | "bs" => {
                Some(KeyEvent::new(KeyCode::Backspace, KeyModifiers::NONE))
            }
            "delete" | "del" => Some(KeyEvent::new(KeyCode::Delete, KeyModifiers::NONE)),
            "space" => Some(KeyEvent::new(KeyCode::Char(' '), KeyModifiers::NONE)),
            "up" => Some(KeyEvent::new(KeyCode::Up, KeyModifiers::NONE)),
            "down" => Some(KeyEvent::new(KeyCode::Down, KeyModifiers::NONE)),
            "left" => Some(KeyEvent::new(KeyCode::Left, KeyModifiers::NONE)),
            "right" => Some(KeyEvent::new(KeyCode::Right, KeyModifiers::NONE)),
            "pgup" => Some(KeyEvent::new(KeyCode::PageUp, KeyModifiers::NONE)),
            "pgdown" => Some(KeyEvent::new(KeyCode::PageDown, KeyModifiers::NONE)),
            "home" => Some(KeyEvent::new(KeyCode::Home, KeyModifiers::NONE)),
            "end" => Some(KeyEvent::new(KeyCode::End, KeyModifiers::NONE)),
            _ => None,
        };
    }

    // Single character
    if s.len() == 1 {
        let ch = s.chars().next()?;
        let modifiers = if ch.is_uppercase() {
            KeyModifiers::SHIFT
        } else {
            KeyModifiers::NONE
        };
        return Some(KeyEvent::new(KeyCode::Char(ch), modifiers));
    }

    None
}
