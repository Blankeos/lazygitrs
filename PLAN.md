[[Prompt]]
Rebuild exactly what `_tmp_lazygit/` (originally in go) is but in rust, hence it's called lazygitrs.

I also mainly want to add this:
https://github.com/jesseduffield/lazygit/pull/5395

My expectation that it's as feature-complete as the original lazygit. But it's faster, more memory efficient, and organized as a good rust codebase.

Make sure to use ratatui. I also added `_tmp_lumen/` in here as a reference because I want to simplify the pager that lazygit currently has to just strictly what lumen has currently. Some good wins there, it has syntax highlighting, it's side-by-side diff viewer by default.

## Implementation Plan

### Architecture Overview

lazygitrs mirrors the original lazygit's architecture but uses Rust idioms and crates:

| Go (lazygit)          | Rust (lazygitrs)                        |
|-----------------------|-----------------------------------------|
| gocui                 | ratatui + crossterm                     |
| go-git                | git2 (libgit2 bindings) + git CLI       |
| goroutines + mutexes  | tokio tasks + Arc<Mutex<>>              |
| yaml.v3               | serde + serde_yaml                      |
| afero (fs)            | std::fs (no abstraction needed)         |
| logrus                | tracing                                 |
| tree-sitter (lumen)   | tree-sitter (for syntax highlighting)   |
| similar (lumen)       | similar (for side-by-side diffs)        |

### Core Crates

```toml
[dependencies]
ratatui = "0.29"
crossterm = "0.28"
git2 = "0.19"
tokio = { version = "1", features = ["full"] }
serde = { version = "1", features = ["derive"] }
serde_yaml = "0.9"
serde_json = "1"
similar = "2"
tree-sitter = "0.24"
tree-sitter-rust = "0.23"
tree-sitter-typescript = "0.23"
tree-sitter-javascript = "0.23"
tree-sitter-python = "0.23"
tree-sitter-go = "0.23"
tree-sitter-json = "0.24"
tree-sitter-bash = "0.23"
tree-sitter-toml = "0.6"
tree-sitter-css = "0.23"
tree-sitter-html = "0.23"
tree-sitter-markdown = "0.4"
tracing = "0.1"
tracing-subscriber = "0.3"
clap = { version = "4", features = ["derive"] }
dirs = "6"
anyhow = "1"
thiserror = "2"
notify-debouncer-mini = "0.5"
unicode-width = "0.2"
```

---

### Module Structure

```
src/
├── main.rs                    # Entry point, CLI parsing, app bootstrap
├── app.rs                     # App struct, run loop, shutdown
├── config/
│   ├── mod.rs                 # Config loading (global + repo-local)
│   ├── user_config.rs         # UserConfig struct (serde, YAML)
│   ├── keybindings.rs         # KeybindingConfig (customizable keys)
│   ├── theme.rs               # Theme definitions + auto-detect
│   └── app_state.rs           # Persisted app state (recent repos, etc.)
├── git/
│   ├── mod.rs                 # GitCommands facade (like Go's git.go)
│   ├── branch.rs              # Branch operations
│   ├── commit.rs              # Commit operations
│   ├── diff.rs                # Diff generation
│   ├── file.rs                # Working tree file operations
│   ├── rebase.rs              # Interactive rebase
│   ├── remote.rs              # Remote operations (fetch, push, pull)
│   ├── stash.rs               # Stash operations
│   ├── tag.rs                 # Tag operations
│   ├── bisect.rs              # Bisect operations
│   ├── worktree.rs            # Worktree operations
│   ├── submodule.rs           # Submodule operations
│   ├── flow.rs                # Git flow operations
│   ├── status.rs              # Repo status
│   ├── config.rs              # Git config reader
│   ├── ai_commit.rs           # AI commit message generation (PR #5395)
│   └── loader/
│       ├── mod.rs
│       ├── branches.rs        # Branch list loader
│       ├── commits.rs         # Commit history loader
│       ├── commit_files.rs    # Files changed in a commit
│       ├── files.rs           # Working tree file loader
│       ├── reflog.rs          # Reflog loader
│       ├── remotes.rs         # Remote loader
│       ├── stash.rs           # Stash entry loader
│       ├── tags.rs            # Tag loader
│       └── worktrees.rs       # Worktree loader
├── model/
│   ├── mod.rs                 # Model struct (all repo data)
│   ├── branch.rs              # Branch model
│   ├── commit.rs              # Commit model (status enum, etc.)
│   ├── file.rs                # File model (tracked, staged, etc.)
│   ├── remote.rs              # Remote + RemoteBranch models
│   ├── stash.rs               # StashEntry model
│   ├── tag.rs                 # Tag model
│   ├── worktree.rs            # Worktree model
│   └── author.rs              # Author model
├── gui/
│   ├── mod.rs                 # Gui struct, main render loop
│   ├── views.rs               # View definitions (all panels)
│   ├── layout.rs              # Layout calculation (panel sizes, splits)
│   ├── context/
│   │   ├── mod.rs             # Context tree + context manager
│   │   ├── branches.rs        # Branches context
│   │   ├── commits.rs         # Commits context
│   │   ├── files.rs           # Files context
│   │   ├── stash.rs           # Stash context
│   │   ├── remotes.rs         # Remotes context
│   │   ├── tags.rs            # Tags context
│   │   ├── status.rs          # Status context
│   │   ├── staging.rs         # Staging context (hunk-level)
│   │   ├── commit_files.rs    # Commit files context
│   │   ├── worktrees.rs       # Worktrees context
│   │   └── submodules.rs      # Submodules context
│   ├── controller/
│   │   ├── mod.rs             # Controller trait + dispatch
│   │   ├── branches.rs        # Branch actions (checkout, merge, rebase, etc.)
│   │   ├── commits.rs         # Commit actions (amend, reword, squash, etc.)
│   │   ├── files.rs           # File actions (stage, unstage, discard, etc.)
│   │   ├── stash.rs           # Stash actions
│   │   ├── remotes.rs         # Remote actions
│   │   ├── tags.rs            # Tag actions
│   │   ├── commit_message.rs  # Commit message editor + AI generation
│   │   ├── merge_conflicts.rs # Merge conflict resolution
│   │   ├── rebase.rs          # Interactive rebase controller
│   │   ├── bisect.rs          # Bisect controller
│   │   ├── undo.rs            # Undo/redo controller
│   │   ├── custom_commands.rs # User-defined custom commands
│   │   ├── filtering.rs       # Filter by path/author
│   │   ├── search.rs          # Search within views
│   │   ├── patch_building.rs  # Patch building mode
│   │   └── worktrees.rs       # Worktree actions
│   ├── popup/
│   │   ├── mod.rs             # Popup manager
│   │   ├── confirm.rs         # Confirmation dialog
│   │   ├── prompt.rs          # Text input prompt
│   │   ├── menu.rs            # Menu/option list
│   │   └── search.rs          # Search input
│   ├── modes/
│   │   ├── mod.rs
│   │   ├── cherry_pick.rs     # Cherry-pick mode
│   │   ├── diffing.rs         # Diffing mode
│   │   ├── filtering.rs       # Filtering mode
│   │   └── marked_base.rs     # Marked base commit mode
│   └── presentation/
│       ├── mod.rs
│       ├── branches.rs        # Branch list rendering
│       ├── commits.rs         # Commit list rendering (graph!)
│       ├── files.rs           # File list rendering
│       ├── stash.rs           # Stash list rendering
│       ├── remotes.rs         # Remote list rendering
│       └── tags.rs            # Tag list rendering
├── pager/
│   ├── mod.rs                 # Diff pager (lumen-inspired)
│   ├── side_by_side.rs        # Side-by-side diff rendering
│   ├── diff_algo.rs           # Diff algorithm (similar crate)
│   ├── highlight/
│   │   ├── mod.rs             # Syntax highlighter (tree-sitter)
│   │   ├── config.rs          # Language configs
│   │   └── queries.rs         # Tree-sitter queries per language
│   ├── theme.rs               # Diff color themes
│   └── word_diff.rs           # Word-level inline diffs
├── os/
│   ├── mod.rs                 # OS command execution
│   ├── cmd.rs                 # Command builder + runner
│   └── platform.rs            # Platform-specific (open, copy, edit)
├── i18n/
│   ├── mod.rs                 # Translation system
│   └── en.rs                  # English strings
└── utils/
    ├── mod.rs
    ├── string.rs              # String utilities
    └── color.rs               # Color utilities
```

---

### Phase 1: Foundation (Skeleton + Git Core)

**Goal:** Bootable TUI that shows repo status, file list, and basic navigation.

1. **CLI & Bootstrap** (`main.rs`, `app.rs`)
   - clap-based CLI: `--path`, `--debug`, `--version`, `--git-dir`, `--work-tree`
   - Config loading from `~/.config/lazygit/config.yml` (reuse existing lazygit configs)
   - App struct with run loop, terminal setup/teardown (crossterm raw mode, alternate screen)

2. **Config System** (`config/`)
   - `UserConfig` with serde_yaml deserialization
   - Key fields: gui, git, keybindings, os, customCommands
   - Load hierarchy: defaults → global config → repo-local config
   - `AppState` persistence (recent repos, last panel)

3. **Git Operations Core** (`git/`, `model/`)
   - `GitCommands` facade struct with sub-command modules
   - OS command runner (`os/cmd.rs`) — builder pattern wrapping `std::process::Command`
   - Loaders: files, branches, commits (initial set)
   - Models: File, Branch, Commit, StashEntry with proper enums

4. **Basic TUI** (`gui/`)
   - ratatui terminal setup with crossterm backend
   - Panel layout: sidebar panels (status, files, branches, commits, stash) + main diff area
   - Context system: trait-based, each panel is a context with its own keybindings
   - Basic navigation: tab between panels, j/k scroll, enter to select
   - Status bar with keybinding hints

**Deliverable:** Can open a repo, see files/branches/commits, navigate between panels.

---

### Phase 2: Git Operations + File Management

**Goal:** Full file staging, committing, and branch operations.

1. **File Operations**
   - Stage/unstage files (whole file, `git add`/`git reset`)
   - Hunk-level staging (parse diff, stage individual hunks)
   - Discard changes (with confirmation)
   - Open in editor, ignore file
   - Rename/delete tracking

2. **Commit Operations**
   - Create commit (popup editor)
   - Amend commit
   - Reword commit message
   - Create fixup/squash commits

3. **Branch Operations**
   - Create, delete, rename branches
   - Checkout branch (with uncommitted changes handling)
   - Merge branch (with conflict detection)
   - Rebase onto branch

4. **Diff Pager** (`pager/`) — lumen-inspired
   - Side-by-side diff as default view
   - `similar` crate for diff computation
   - tree-sitter syntax highlighting (Rust, TS, JS, Python, Go, JSON, etc.)
   - Word-level inline diffs for modified lines
   - Scrollable with keyboard + mouse
   - Context lines showing function signatures
   - Fullscreen toggle (side-by-side ↔ old-only ↔ new-only)

**Deliverable:** Can stage files, create commits, manage branches, view beautiful diffs.

---

### Phase 3: Advanced Git + Interactive Rebase

**Goal:** Interactive rebase, cherry-pick, stash, and remote operations.

1. **Interactive Rebase**
   - Reorder commits (move up/down)
   - Squash, fixup, drop, edit, reword
   - Conflict resolution during rebase
   - Abort/continue/skip rebase

2. **Cherry-Pick Mode**
   - Select commits for cherry-pick (multi-select)
   - Apply cherry-picks with conflict handling

3. **Stash Operations**
   - Stash all, stash staged, stash with message
   - Pop, apply, drop stash entries
   - View stash contents in pager

4. **Remote Operations**
   - Fetch, pull, push
   - Force push (with confirmation)
   - Set upstream tracking
   - Remote management (add, remove, edit)

5. **Tag Operations**
   - Create lightweight + annotated tags
   - Delete tags, push tags

6. **Bisect**
   - Start/reset bisect
   - Mark good/bad
   - Visual bisect progress

**Deliverable:** Full interactive rebase, cherry-pick flow, remotes, stash, tags, bisect.

---

### Phase 4: Polish + Advanced Features

**Goal:** Feature parity with lazygit + the extras.

1. **AI Commit Messages** (`git/ai_commit.rs`) — PR #5395
   - Config: `git.commit.generateCommand` (string, shell command)
   - Pipes `git diff --cached` via **stdin** to the configured command
   - Parses stdout, stripping markdown code fences if present
   - Populates commit message editor with AI output
   - Triggered from commit menu (`<c-o>` then `g`)
   - Shows "Generating commit message..." state, blocks editing while running
   - Disabled with hint message when `generateCommand` is empty
   - Provider-agnostic: works with any CLI (claude, opencode, ollama, etc.)
   - Example: `generateCommand: "claude -p 'Generate a conventional commit message for this diff:'"`

2. **Patch Building Mode**
   - Select lines/hunks from commits to build custom patches
   - Apply patch to index, working tree, or new commit

3. **Custom Commands**
   - User-defined commands in config
   - Template variables (selected branch, commit, file, etc.)
   - Prompt support, confirmation dialogs

4. **Undo/Redo**
   - Reflog-based undo for destructive operations
   - Visual undo stack

5. **Submodule Support**
   - View, update, init submodules
   - Enter submodule repo

6. **Worktree Support**
   - List, create, remove worktrees
   - Switch between worktrees

7. **Search & Filter**
   - Filter commits by path, author
   - Search within any list view
   - Fuzzy finding

8. **Mouse Support**
   - Click to select items
   - Scroll with mouse wheel
   - Click panel to focus

9. **Command Log**
   - Show all git commands being run
   - Toggleable panel

10. **Multi-repo**
    - Recent repos list
    - Quick switch between repos
    - Per-repo state persistence

---

### Phase 5: Commits View Ergonomics

**Goal:** Better UX for the commits panel — richer graph, filtering, and search.

1. **Better Graph View** (on by default)
   - Enhanced commit graph visualization in the commits panel
   - Clear branch/merge topology rendering

2. **Filter by Branch**
   - Filter commit list to show only commits from a specific branch
   - Quick branch selector for filtering

3. **Filter by Commit Message**
   - Search/filter commits by message text
   - Useful for conventional commit workflows (e.g., filter by ticket name/ID)

---

### Phase 6: Testing & Hardening

1. **Unit Tests**
   - Git operation wrappers
   - Config parsing
   - Diff algorithm
   - Model serialization

2. **Integration Tests**
   - Inspired by lazygit's test framework
   - Headless TUI driver (send keys, assert view content)
   - Test categories matching lazygit: branch, commit, rebase, stash, etc.
   - Snapshot-based diff testing for pager output

3. **Performance**
   - Profile large repos (10k+ commits)
   - Lazy loading for commit history
   - Async refresh (don't block UI on git operations)
   - Benchmark against Go lazygit

4. **Error Handling**
   - Graceful git error display
   - Recovery from failed operations
   - Terminal restore on panic (crossterm cleanup hook)

---

### Key Design Decisions

1. **git2 + CLI hybrid**: Use `git2` (libgit2) for read-heavy operations (log, diff, status) for speed. Fall back to `git` CLI for write operations and complex flows (rebase, merge) to ensure exact behavioral parity with Git.

2. **Lumen-style pager over lazygit's pager**: The diff view defaults to side-by-side with tree-sitter syntax highlighting and word-level diffs, matching lumen's approach. This is a strict improvement — no need to port lazygit's simpler pager.

3. **Config compatibility**: Read lazygit's existing `~/.config/lazygit/config.yml` format so users can migrate without reconfiguring. Add `generateCommand` as a new field under `git.commit`.

4. **Async-first**: Use tokio for all git operations that might block. The TUI render loop runs on the main thread; git operations happen on background tasks with channel-based result delivery.

5. **Trait-based contexts**: Each panel context implements a `Context` trait with methods for keybindings, rendering, and state. Controllers are separate structs that handle actions. This replaces lazygit's interface-based approach with Rust traits.

6. **No CGO dependency**: While git2 uses libgit2 (C), it's statically linked. The binary is fully self-contained.
