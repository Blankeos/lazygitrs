# Changelog

All notable changes to this project will be documented in this file.

## [0.0.17] - 2026-04-27

### Bug Fixes

- Enable clipboard paste in cmdk dialogs.
- Make push/pull shortcuts work globally, including in diff panel

### Features

- Add quick action to checkout previous branch.
- Persist commit editor text on Esc and add Clear option.
- Add copy and open shortcuts to branch commits and reflog.
- Display files in Zed-style format with filename prominent.

## [0.0.16] - 2026-04-27

### Bug Fixes

- Handle shift+enter to insert newlines in body.
- Textarea wrapping and terminal state on resize/panic.

### Features

- Add visual-line aware keyboard shortcuts for soft-wrapped text.
- Add repo URL and contributors to status view.
- Implement soft-wrapping for commit body input.
- Enable instant text pasting in inputs.
- Add branch name copy option in branches view.

## [0.0.15] - 2026-04-22

### Bug Fixes

- Made the commit panel visibility by (.) save to state.

## [0.0.14] - 2026-04-22

### Bug Fixes

- Decode porcelain-quoted paths from git status.

### Chores

- Allow-dirty for custom release.yml
- Added git cliff for changelogs.

### Documentation

- Updated readme.

### Features

- Add configurable refresher with periodic background auto-fetch.
- Working commit details panel with toggle.

## [0.0.13] - 2026-04-09

### Features

- Add loading state indicators for async menu item actions.
- Enable drill-down navigation and display current branch.

## [0.0.12] - 2026-04-03

### Bug Fixes

- Enable mouse scroll for list views in split screen mode.

### Documentation

- Added demo pics.

### Features

- Preserve manual viewport scrolling when selection changes.
- Better sub-view click behavior.
- Diff_mode async mode performance improvements.

### Refactor

- Extract scroll logic and improve viewport scrolling.

## [0.0.11] - 2026-04-01

### Bug Fixes

- Ensure minimum padding in status sidebar when changes are displayed.

### Documentation

- Finished todo notes.

### Features

- Add click support for popups and refactor scroll offset management.

## [0.0.10] - 2026-04-01

### Bug Fixes

- Adjust color theme picker dimensions for better layout.

### Chores

- Install script for linux.
- Better tag and release.
- Cargo lock.

### Documentation

- Updated todos.
- Sample todos change.
- Themes!

### Features

- Batch file staging and improve untracked file diffs.
- Better indentation + allow discarding all changes in a directory from tree view.
- HUGE PERF IMPROVEMENT  parse diffs on background threads with loading indicator.
- Add version in status.
- Add XDG_STATE_HOME support for state directory separation.
- Add 30+ built-in themes and JSON-based custom theme support.
- Add 13 built-in color themes with theme picker.
- Add mouse support for combobox and prioritize current branch.
- Remove redundant root node with single child.
- Compress single-child directory chains into combined paths.

### Refactor

- Add state_dir and use it for state files.
- Consolidate list picker logic with shared ListPickerCore.

## [0.0.9] - 2026-03-28

### Bug Fixes

- Correct body textarea height calculation to account for padding.

### Features

- Add two-field commit message editor with summary and body.
- Multi-commit cherry-picking with range selection
- Audited help.

## [0.0.8] - 2026-03-27

### Chores

- Fix readme.

### Documentation

- Readme fixes.

### Features

- Support editing in multi-file diffs.
- Emit config commands in the command log.
- Add column number support to edit-at-line.

## [0.0.6] - 2026-03-26

### Bug Fixes

- Tui-textarea for `?` help dialog.
- Preserve scroll position during file reload

### Chores

- Todos done.
- Updated todos.
- Todos and stuff.

### Documentation

- Sync readme.
- More stuff in readme.

### Features

- Better commit experience, better help aesthetics.
- Working interactive rebase. (with some rough edges).
- Add search functionality to diff view.
- Ref options now work.
- Add MessageKind enum and refactor message popups
- Add remote operation indicator on head branch
- Shift + scroll up/down to horizontally scroll.
- Horizontal scroll w/ mouse on the diff viewer.

### Refactor

- Cleaner-looking files.

## [0.0.5] - 2026-03-25

### Bug Fixes

- Discard doesn't work.

## [0.0.4] - 2026-03-25

### Bug Fixes

- Delete diff count taking space.
- Properly do strikethroughs.

### Fi

- Select highlights.

## [0.0.3] - 2026-03-25

### Documentation

- Added working startup benchmarks w/ hyperfine.

## [0.0.2] - 2026-03-25

### Chores

- Just tag and release tweaks.

### Documentation

- Synced readme.

## [0.0.1] - 2026-03-25

### Bug Fixes

- Popup space taking and colors.
- Diff generation for merge commits and stashes

### Chores

- License to mit.
- Ready for release.
- Added logo in -- --help

### Documentation

- Add project TODO tracking file
- Better docs!
- Updated plan.
- Readme stuff.

### Features

- Add simple Message popup variant for informational alerts
- Add clipboard and browser menu shortcuts to branches help
- Added reflog.
- Preserve side-by-side view mode across reloads
- Add mouse text selection to diff view with copy support
- Add interactive help popup with searchable keybindings
- Add multi-file diff support with per-file syntax highlighting
- Add branch commits view navigation
- Add stash file viewing with Enter key
- Add commit file viewer for browsing files changed in a commit
- Distinguish HEAD commit with solid circle in graph
- Phase 5 (graphy graph).
- Graph and --all.
- Add keyboard enhancement flags and improve key handling
- Add search textarea, fix popup keybindings, and rename project
- Add interactive diff navigation and polish README
- Slash fills on side by side diffs.
- Add file tree view with collapsible directories
- Optimizations in commit list item scroll.
- Some enhancements to make it look better.
- Phase 3.
- Phase 2.
- Phase 1.
- Initial plan and analysis.

### Refactor

- Copy to clipboard menu helperentries.


