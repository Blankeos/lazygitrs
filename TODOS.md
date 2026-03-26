- [x] commits pane overhaul
  - [x] Better graph view (enabled by default)
  - [x] Filter by branch
  - [x] Filter by commit message (handy if you prefix with ticket IDs)
- [ ] ~Command palette (OpenCode-style) — still figuring this one out~

- [x] Stash viewer:
  - Can we add the same viewer for files in the '[5] Stash' sidebar tab? (the same way we currently do with Commits tab)
- [x] Enter key in the branches sidebar tab.
  - When I press 'enter' here, it shows the 'Commits (<branch>)'. Then when I press 'enter' again it shows the 'Commit Files' (kinda similar to the [4] commits sidebar tab)
- [x] Commit item focus, what does the diff preview look like? Currently it's just plain (which kinda makes sense because there could be multiple of files in 1 commit) Expected: A nice viewer wherein I can still see the syntax highlighting, just as nice as hovering on a single file. I wonder if we can still use similar for this.
- [x] A 'Help' sort of 'which-key' feature thingy, pressing `?` would open a dialog that shows me which keys I can press in the current context. Make it also searchable i.e. pressing `/` would highlight the specific hotkey I'm looking for. Very similar to the original lazygit, just make it look better because I actually didn't like the original. i.e. The search looked too disconnected from the dialog.
- [x] Like \_tmp_lumen, I want to be able to highlight lines on the diff exploration viewing mode with my mouse.. Highlighting something would also show the same `y copy esc` tooltip just under the highlight. (no annotate since that's a lumen concept)
- [x] Like \_tmp_lumen, I want to `{}` to travel between hunks. I want `[]` to show 'old' or 'new' (so it toggle hides the side-by-side). Make sure the `[]` doesn't break the mouse interactions for highlights. I want to show the `?` help panel while focusing the 'main content diff content exploration focus' so I can see these hotkeys.
- [x] Like the original lazygit, let's have a subtab under [4] Commits, for Reflog.

- [x] More feature-parity stuff with the original lazygit... Missing features from the original lazygit (from my investigation, but I could be missing more, so add more here)
  - [x] In 'Remotes', I press `n`, prolly not implemented.
  - [x] In 'Remotes', I press `d` (delete), prolly not implemented.
  - [x] In 'Tags', I press `g` (reset), prolly not implemented.
    - [x] I noticed in the original lazygit, in the reset menu options, I see the associated command w/ it i.e.
    - Mixed reset reset --mixed f115cxxx (the 'reset --mixed f115cxxx' has a different color.)
    - Soft reset reset --soft f115cxxx
    - Hard reset reset --hard f115cxxx
  - [x] In 'Tags', I press `P`, to push tags? prolly not implemeanted.
  - [x] contextual `?` for some other pages that we haven't considered before.
    - [x] I press `?` on Remotes, I don't see much.
    - [x] I press `?` on Tags, I don't see much.
    - [x] I press `?` on Worktrees, I don't see much. It still says 'Files'
    - [x] I press `?` on Submodules, I don't see much. It still says 'Files'
  - [x] In Tags, in the original lazygit, I can:
    - [x] Press enter and see a 'commits list view'?
    - [x] after in the 'commits list view', I can press enter again and see the 'commit files' view.
    - [x] after in the 'commit files' view, I can press enter to go into 'diff exploration' (if you notice this is pretty much all standard at this point)
  - [x] In Reflog, in the original lazygit, I can:
    - [x] Press enter and it goes into 'commits list view', then if I press enter it goes into 'commit files' view, and enter again goes to 'diff exploration' (pretty standard again)
  - [x] In 'commits list view', I can press `o` to open commit in the browser.
    - Let's make this a bit different for lazygitrs. Same idea with the 'Branches' `o` key. It opens a popup for 'Open in browser' with a list of stuff I can open about this commit. So I guess 1 option is just the 'Open commit url'
    - Actually now that I realize.. We already have a `y` option for Commit url, so that's very good.
  - [x] In 'Commits list view', pressing `y` opens the 'Copy to clipboard'. Minor issue/changes:
    - [x] In the original lazygit, sometimes 'commit message body' is strikethrough'd Maybe because if it doesnt exist?
    - [x] In the original lazygit, sometimes 'commit tags' is strikethrough'd Maybe because it doesnt exist?

- [ ] For the feature-parity stuff I didn't consider in the previous todo, write it here (For AI):
  - Interactive Rebase / Commit Manipulation:
    - [ ] In 'Commits', I press `d` to drop the selected commit. Currently unimplemented.
    - [ ] Cherry-pick paste (`V`) — we have cherry-pick copy (`C`) in Commits, but no paste action to apply copied commits onto current branch.
    - [ ] In 'Commits', the original lazygit has `<c-r>` to reset cherry-pick selection.
    - [ ] Undo/Redo — the original lazygit has `z`/`<c-z>` to undo and redo git actions (using reflog under the hood).
  - Conflict Resolution:
    - [ ] Merge conflict resolution UI — the original lazygit lets you pick between versions when a merge/rebase results in conflicts.
    - [ ] Rebase conflict resolution UI — similar conflict resolution flow during interactive rebase.
    - [ ] In 'Files', the original lazygit has `M` to open merge tool / external merge tool for resolving conflicts.
  - Files:
    - [x] In 'Files', the original lazygit has `e` to open file in editor and `o` to open file in default program.
    - [x] In 'Files', the original lazygit has `<c-o>` to copy the diff of the selected file to clipboard (we have this in `y` menu, but the direct shortcut may be missing).
    - [ ] Full `$EDITOR` integration — committing with `C` (editor mode) currently has a limitation where it can't suspend the TUI to open a real terminal editor.
  - Remotes:
    - [x] In 'Remotes', pressing `Enter` should drill into remote branches. Then from a remote branch: `<space>` to checkout, `M` to merge, `r` to rebase onto it, `d` to delete remote branch.
  - Submodules:
    - [x] In 'Submodules', the original lazygit has more operations: `a` to add submodule, `d` to remove submodule, `e` to enter submodule (open a nested lazygit in that submodule), `<space>` to update submodule.
  - Worktrees:
    - [x] In 'Worktrees', the original lazygit has `<space>` to switch to worktree (open it).
  - Branches:
    - [x] In 'Branches', the original lazygit shows divergence info (ahead/behind counts relative to upstream). (already implemented)
  - Done / Won't Do:
    - [x] ~Diff mode — the original lazygit has a way to diff any two commits/branches against each other (not just viewing a single commit's diff).~ (Author check: I have separate ideas for diff mode: comapring two commits/branches against each other, it'll be more intuitive)
    - [x] ~In 'Branches', the original lazygit has `<c-o>` to copy PR URL, we might already have this in the `y` menu but worth verifying the direct shortcut.~ (Author check: so yeah we won't need this)

- [x] Improve the speeds still, very important for larger repos. Improve first-load speed. Either cache the data, or the render the TUI even before the git load model data isn't there yet. (perceived speed)
- [x] regular push behavior to essentially do `git push origin HEAD`

- [ ] Config-parity, make sure everything works.
- [ ] Hot reloading of config (I can edit the config on the fly and the config is still read without restarting lazygit)
- [x] Bug: in the diff exploration view, because of the 10s interval I think the position of which I scrolled at also seems to get reset. Ideally not. Just like how the [new] and [old] -- it used to have this bug but I fixed it.
- [ ] Search feature inside the diff exploration view is much needed. Grep for all in diff_mode is good too.
- [ ] In `?` help dialog, use tui-textarea so I can erase the input using opt-backspace.

## Stuff I wanna do differently

- [ ] Interactive Rebase should be more intuitive.
  - [ ] I can see a commits list and then also see the commit it'll be merging into. Kinda exactly like VSCode's interactive rebase editor. https://user-images.githubusercontent.com/641685/102309169-31ba2a00-3f36-11eb-8b26-050c7d83fa3f.png but in TUI version. This could be a dialog on its own with its own focus groups. It'll look simpler and more interactive than the current lazygit.
    - Non-negotiables for me are:
      - I can press jk up down to switch between commits. I can h l left right to change the value to pick, squash, drop, edit.
      - The pick, drop, edit, squash options have semantic colors. The same w/ VSCode.
      - The node-like colors w/ indicators on the left side are great to have.
      - I can SEE the commit it'll rebase ontop of i.e. 'Hello GitLens' in this example.
      - I can see a 'Start Rebase' and an 'Abort' action.
- [x] Diff Mode / Compare Mode
  - Diff mode can be opened w/ a commad palette or when focusing on either BRANCH or COMMITS tab.
  - First trigger of it opening will open its own sort of screen that looks like:

    ```
    -------------------------------------------------------
    | A: ccf0183  | B: 09s8c90 |                          |
    ---------------------------- diff exploration view    |
    | Commit Files             |                          |
    |                          |                          |
    |                          |                          |
    |                          |                          |
    -------------------------------------------------------
    ```

    - So there's like an A and B comboboxes there. They can help you autosearch for a commit or a branch.
    -
    - You can obviously exit and go back to the default lazygit UI.
    - You can press tab to cycle focus between the A and B comboboxes, Commit Fles, and diff exploration view.
    - Commit Files and Diff Exploration View actually already exist if you notice. So as expected, they'd have the same hotkeys sort of. Especially diff exploration view like `[]` `{}`.
