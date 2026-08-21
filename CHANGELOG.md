# Changelog

All notable changes to this project will be documented in this file.

## [0.0.33] - 2026-08-21

### Features

- Add edit and open-file shortcuts to commit files panel by @Blankeos
- Support suspending TUI for terminal editors (hx/nvim/vim) by @Blankeos
- Display co-authors in commit details and offer branch creation on checkout miss by @Blankeos

## [0.0.32] - 2026-08-13

### Features

- Add remote edit and fork-remote workflows by @Blankeos
- Add lazygitrs self-upgrade command by @Blankeos

### Performance

- Make commit diff navigation instant with prefetch and churn fix (#24) by @joshxfi in [#24](https://github.com/Blankeos/lazygitrs/pull/24)


### New Contributors

- @joshxfi made their first contribution in [#24](https://github.com/Blankeos/lazygitrs/pull/24)
## [0.0.31] - 2026-08-13

### Bug Fixes

- Errors when partially adding filetrees by @Blankeos
- Migrate checklist search input to textarea with enhanced editing by @Blankeos
- Render pure rename diffs by falling back to file content by @Blankeos
- Prevent concurrent fetch races causing divergent branch errors by @Blankeos

### Features

- Skip expensive untracked file stats in diff/file loading by @Blankeos
- Fix tree row spacing for status icons by @Blankeos

### Performance

- Optimize stage-all toggling with optimistic UI updates by @Blankeos
- Make stage/unstage and checkout paths non-blocking by @Blankeos

## [0.0.30] - 2026-08-05

### Features

- Add list search match highlighting in rendered panels by @Blankeos
- Support multi-author commit filtering by @Blankeos
- Ctrl-s, add commit filtering by branch, path, and author by @Blankeos
- Stream HEAD metadata during model loading by @Blankeos

## [0.0.29] - 2026-08-02

### Bug Fixes

- Show placeholder for binary files in diff views by @Blankeos
- Include `--root` for root-commit file diffs by @Blankeos
- Fix portrait sidebar resize hit-testing and ratio mapping by @Blankeos
- Restore syntax highlighting for multi-file commit previews by @Blankeos

### Features

- Migrate help overlays to executable command palettes by @Blankeos
- Add global reset picker and share reset actions by @Blankeos
- Support AI commit message generation for commit rewording by @Blankeos

### Performance

- Skip commit file stats when loading stash contents by @Blankeos
- Batch directory diffs and parallelize multi-file highlighting by @Blankeos

## [0.0.28] - 2026-07-27

### Bug Fixes

- Avoid overwriting commits while branch filter is active by @Blankeos
- Harden terminal input handling and isolate background commands (#23) by @Blankeos in [#23](https://github.com/Blankeos/lazygitrs/pull/23)
- Render commit graph with per-row width and single separator space by @Blankeos

### Chores

- Added automated npm/cratesio publishing by @Blankeos

### Features

- Add draggable sidebar divider interaction by @Blankeos
- Add popup mouse selection + click-to-activate behavior by @Blankeos
- Replace remote operation modal with lightweight loading toast by @Blankeos
- Display remote tag presence and gate remote deletion by @Blankeos
- Show co-located branch labels on tags by @Blankeos
- Allow rewording empty commits by @Blankeos
- Show async Loading modal during remote operations by @Blankeos

### Performance

- Run commit actions through async ops and streaming refresh by @Blankeos

## [0.0.27] - 2026-07-23

### Bug Fixes

- Handle fragmented escape sequences and suppress command modifiers by @Blankeos

### Performance

- Optimize async diff loading and commit detail fetching by @Blankeos

## [0.0.26] - 2026-07-23

### Bug Fixes

- Preserve terminal text input when enabling keyboard enhancements by @Blankeos

## [0.0.25] - 2026-07-22

### Bug Fixes

- #20 hover disabled optimistic so it doesn't hit ghostty users by @Blankeos
- Persist file-tree visibility in compare mode by @Blankeos

### Features

- Add branch-aware commit checkout menu by @Blankeos

## [0.0.24] - 2026-07-12

### Bug Fixes

- Force plain unified output for stash diff by @Blankeos
- Ignore no-newline metadata in unified diff parsing by @Blankeos
- Render unified modified hunks in old-then-new chunk order by @Blankeos

### Features

- Show diff hunk/addition/deletion stats in commit file lists by @Blankeos
- Show current hunk position in side-by-side header by @Blankeos
- Show inline diff stats in staged file lists by @Blankeos
- Clarify cherry-pick copy/paste UX by @Blankeos
- Add confirmation before committing from detached HEAD by @Blankeos
- Show diff layout mode in status and help hints by @Blankeos
- Make diff layout toggle keybinding configurable by @Blankeos

### Performance

- Avoid extra branch recency calls and gate auto-refresh on fetch output by @Blankeos

## [0.0.23] - 2026-06-18

### Bug Fixes

- Restore head-marker behavior for merge nodes by @Blankeos
- Resolve previous branch checkout via explicit ref name by @Blankeos
- Handle rename/copy paths consistently across diffs and file actions by @Blankeos

### Chores

- Publish Homebrew formula via release workflow by @Blankeos

## [0.0.22] - 2026-06-10

### Bug Fixes

- Update status-bar hint for diff layout toggle by @Blankeos
- Centralize textarea shortcut handling across input fields by @Blankeos
- Show errors instead of aborting on failure paths by @Blankeos
- Mouse-scroll drain pending crossterm events on terminal restore by @Blankeos

### Features

- Add palette selector + guard popup transitions and bound popup dialog height by @Blankeos
- Add interactive local and remote tag deletion flow by @Blankeos
- Resolve repo root and name from git top-level by @Blankeos
- Support repo-inspection generators and preserve draft on failure by @Blankeos

## [0.0.21] - 2026-05-31

### Bug Fixes

- Fix unified diff line-to-side mapping for edit actions by @Blankeos
- Guard command log layout against tiny terminal panels. by @Blankeos
- Treat conflict pauses as success and keep progress view in sync. by @Blankeos
- Make mouse scroll work during description focus in commit message modal. by @Blankeos

### Features

- Add toggleable unified diff mode and improve PR URL fallback resolution by @Blankeos
- Paginate commit history loading for large repos. by @Blankeos
- Add cancellable AI commit generation and async remote ops by @Blankeos
- Resize side/diff panel width (#18) by @fcmiranda in [#18](https://github.com/Blankeos/lazygitrs/pull/18)
- Add cherry-pick functionality with keybindings and handling options by @fcmiranda

### Refactor

- Rustfmt, and always rustfmt (big diff, warning!). by @Blankeos

## [0.0.20] - 2026-05-16

### Bug Fixes

- Fixed for wrap mode (revert shows). by @Blankeos
- Inaccurate index-based hunk reverting. by @Blankeos
- Preserve wrap preference when resetting diff view state. by @Blankeos
- Pressing enter does shift+enter by @Blankeos
- Fix `add` for `git mv` (moved) files. by @Blankeos
- Handle unstage_all when no commits exist. by @carlo-bilby
- Prevent cross-hunk aliasing in side-by-side diff. by @Blankeos

### Chores

- Cleanup before merge. by @Blankeos

### Features

- Replace dedicated revert keybindings with {/} hunk cycling and context menu. by @Blankeos
- Added a local undo state stack. by @Blankeos
- Bind <c-r> to revert selected hunk by @Blankeos
- Hover tooltip on revert-hunk marker by @Blankeos
- Revert individual hunks from the diff view by @fcmiranda
- Better graph nodes. by @Blankeos
- Display author name for rebase onto commit. by @Blankeos
- Route s/f/d/e through Interactive Rebase planner by @Blankeos

### Refactor

- Rebind revert-hunk from ctrl+r to enter. by @Blankeos

### Doc

- Minor typo fix. by @Blankeos


### New Contributors

- @carlo-bilby made their first contribution
## [0.0.19] - 2026-04-29

### Bug Fixes

- Enter InProgress view immediately on startup when rebase is detected. by @Blankeos
- Batch entry hydration into single git log invocation. by @Blankeos
- Improve scroll and viewport visibility handling. by @Blankeos
- Add reverse toggle panel keybinding for Shift+Tab by @fcmiranda

### Chores

- Added contribution attribs w/ gitcliff. by @Blankeos

### Documentation

- Clarify paths and add legacy state migration. by @Blankeos
- Added codex command. by @Blankeos

### Features

- Allow creating empty commits when no files present. by @Blankeos
- Highlight key bindings in confirmation dialog. by @Blankeos
- Add interactive ✦ AI-generate button to commit message dialog. by @Blankeos
- Ai commit shortcut (#11) by @fcmiranda in [#11](https://github.com/Blankeos/lazygitrs/pull/11)
- Load from ~/.config/lazygitrs with lazygit fallback (#6) by @fcmiranda in [#6](https://github.com/Blankeos/lazygitrs/pull/6)
- Add gutter color styling to diff display. resolves #10 by @Blankeos


### New Contributors

- @fcmiranda made their first contribution in [#11](https://github.com/Blankeos/lazygitrs/pull/11)
## [0.0.18] - 2026-04-28

### Bug Fixes

- Always show refs and tags in commit details. by @Blankeos
- 'Status' header in fullview to '1 Status'. by @Blankeos
- Add search highlighting to keybindings dialog. by @Blankeos
- Expand Files tab when Status is focused to fill empty space. by @Blankeos
- Reduce minimum height threshold for portrait layout. by @Blankeos
- Preserve file list order when staging files. by @Blankeos

### Chores

- Added MIT license. by @Blankeos

### Features

- Apply Zed-style file display to all commit file contexts. by @Blankeos
- Show full commit messages in full-view details panel. by @Blankeos
- Open editor at focused hunk when navigating diffs with {}. by @Blankeos
- Add e/o key handlers to diff panel for opening files. by @Blankeos
- Open editor at first changed hunk when pressing 'e' on file. by @Blankeos
- Enable vertical layout for half view mode. by @Blankeos

## [0.0.17] - 2026-04-27

### Bug Fixes

- Enable clipboard paste in cmdk dialogs. by @Blankeos
- Make push/pull shortcuts work globally, including in diff panel by @Blankeos

### Features

- Add quick action to checkout previous branch. by @Blankeos
- Persist commit editor text on Esc and add Clear option. by @Blankeos
- Add copy and open shortcuts to branch commits and reflog. by @Blankeos
- Display files in Zed-style format with filename prominent. by @Blankeos

## [0.0.16] - 2026-04-27

### Bug Fixes

- Handle shift+enter to insert newlines in body. by @Blankeos
- Textarea wrapping and terminal state on resize/panic. by @Blankeos

### Features

- Add visual-line aware keyboard shortcuts for soft-wrapped text. by @Blankeos
- Add repo URL and contributors to status view. by @Blankeos
- Implement soft-wrapping for commit body input. by @Blankeos
- Enable instant text pasting in inputs. by @Blankeos
- Add branch name copy option in branches view. by @Blankeos

## [0.0.15] - 2026-04-22

### Bug Fixes

- Made the commit panel visibility by (.) save to state. by @Blankeos

## [0.0.14] - 2026-04-22

### Bug Fixes

- Decode porcelain-quoted paths from git status. by @Blankeos

### Chores

- Allow-dirty for custom release.yml by @Blankeos
- Added git cliff for changelogs. by @Blankeos

### Documentation

- Updated readme. by @Blankeos

### Features

- Add configurable refresher with periodic background auto-fetch. by @Blankeos
- Working commit details panel with toggle. by @Blankeos

## [0.0.13] - 2026-04-09

### Features

- Add loading state indicators for async menu item actions. by @Blankeos
- Enable drill-down navigation and display current branch. by @Blankeos

## [0.0.12] - 2026-04-03

### Bug Fixes

- Enable mouse scroll for list views in split screen mode. by @Blankeos

### Documentation

- Added demo pics. by @Blankeos

### Features

- Preserve manual viewport scrolling when selection changes. by @Blankeos
- Better sub-view click behavior. by @Blankeos
- Diff_mode async mode performance improvements. by @Blankeos

### Refactor

- Extract scroll logic and improve viewport scrolling. by @Blankeos

## [0.0.11] - 2026-04-01

### Bug Fixes

- Ensure minimum padding in status sidebar when changes are displayed. by @Blankeos

### Documentation

- Finished todo notes. by @Blankeos

### Features

- Add click support for popups and refactor scroll offset management. by @Blankeos

## [0.0.10] - 2026-04-01

### Bug Fixes

- Adjust color theme picker dimensions for better layout. by @Blankeos

### Chores

- Install script for linux. by @Blankeos
- Better tag and release. by @Blankeos
- Cargo lock. by @Blankeos

### Documentation

- Updated todos. by @Blankeos
- Sample todos change. by @Blankeos
- Themes! by @Blankeos

### Features

- Batch file staging and improve untracked file diffs. by @Blankeos
- Better indentation + allow discarding all changes in a directory from tree view. by @Blankeos
- HUGE PERF IMPROVEMENT  parse diffs on background threads with loading indicator. by @Blankeos
- Add version in status. by @Blankeos
- Add XDG_STATE_HOME support for state directory separation. by @Blankeos
- Add 30+ built-in themes and JSON-based custom theme support. by @Blankeos
- Add 13 built-in color themes with theme picker. by @Blankeos
- Add mouse support for combobox and prioritize current branch. by @Blankeos
- Remove redundant root node with single child. by @Blankeos
- Compress single-child directory chains into combined paths. by @Blankeos

### Refactor

- Add state_dir and use it for state files. by @Blankeos
- Consolidate list picker logic with shared ListPickerCore. by @Blankeos

## [0.0.9] - 2026-03-28

### Bug Fixes

- Correct body textarea height calculation to account for padding. by @Blankeos

### Features

- Add two-field commit message editor with summary and body. by @Blankeos
- Multi-commit cherry-picking with range selection by @Blankeos
- Audited help. by @Blankeos

## [0.0.8] - 2026-03-27

### Chores

- Fix readme. by @Blankeos

### Documentation

- Readme fixes. by @Blankeos

### Features

- Support editing in multi-file diffs. by @Blankeos
- Emit config commands in the command log. by @Blankeos
- Add column number support to edit-at-line. by @Blankeos

## [0.0.6] - 2026-03-26

### Bug Fixes

- Tui-textarea for `?` help dialog. by @Blankeos
- Preserve scroll position during file reload by @Blankeos

### Chores

- Todos done. by @Blankeos
- Updated todos. by @Blankeos
- Todos and stuff. by @Blankeos

### Documentation

- Sync readme. by @Blankeos
- More stuff in readme. by @Blankeos

### Features

- Better commit experience, better help aesthetics. by @Blankeos
- Working interactive rebase. (with some rough edges). by @Blankeos
- Add search functionality to diff view. by @Blankeos
- Ref options now work. by @Blankeos
- Add MessageKind enum and refactor message popups by @Blankeos
- Add remote operation indicator on head branch by @Blankeos
- Shift + scroll up/down to horizontally scroll. by @Blankeos
- Horizontal scroll w/ mouse on the diff viewer. by @Blankeos

### Refactor

- Cleaner-looking files. by @Blankeos

## [0.0.5] - 2026-03-25

### Bug Fixes

- Discard doesn't work. by @Blankeos

## [0.0.4] - 2026-03-25

### Bug Fixes

- Delete diff count taking space. by @Blankeos
- Properly do strikethroughs. by @Blankeos

### Fi

- Select highlights. by @Blankeos

## [0.0.3] - 2026-03-25

### Documentation

- Added working startup benchmarks w/ hyperfine. by @Blankeos

## [0.0.2] - 2026-03-25

### Chores

- Just tag and release tweaks. by @Blankeos

### Documentation

- Synced readme. by @Blankeos

## [0.0.1] - 2026-03-25

### Bug Fixes

- Popup space taking and colors. by @Blankeos
- Diff generation for merge commits and stashes by @Blankeos

### Chores

- License to mit. by @Blankeos
- Ready for release. by @Blankeos
- Added logo in -- --help by @Blankeos

### Documentation

- Add project TODO tracking file by @Blankeos
- Better docs! by @Blankeos
- Updated plan. by @Blankeos
- Readme stuff. by @Blankeos

### Features

- Add simple Message popup variant for informational alerts by @Blankeos
- Add clipboard and browser menu shortcuts to branches help by @Blankeos
- Added reflog. by @Blankeos
- Preserve side-by-side view mode across reloads by @Blankeos
- Add mouse text selection to diff view with copy support by @Blankeos
- Add interactive help popup with searchable keybindings by @Blankeos
- Add multi-file diff support with per-file syntax highlighting by @Blankeos
- Add branch commits view navigation by @Blankeos
- Add stash file viewing with Enter key by @Blankeos
- Add commit file viewer for browsing files changed in a commit by @Blankeos
- Distinguish HEAD commit with solid circle in graph by @Blankeos
- Phase 5 (graphy graph). by @Blankeos
- Graph and --all. by @Blankeos
- Add keyboard enhancement flags and improve key handling by @Blankeos
- Add search textarea, fix popup keybindings, and rename project by @Blankeos
- Add interactive diff navigation and polish README by @Blankeos
- Slash fills on side by side diffs. by @Blankeos
- Add file tree view with collapsible directories by @Blankeos
- Optimizations in commit list item scroll. by @Blankeos
- Some enhancements to make it look better. by @Blankeos
- Phase 3. by @Blankeos
- Phase 2. by @Blankeos
- Phase 1. by @Blankeos
- Initial plan and analysis. by @Blankeos

### Refactor

- Copy to clipboard menu helperentries. by @Blankeos


### New Contributors

- @Blankeos made their first contribution

