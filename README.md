# lazygitrs

A slopfork of lazygit, but in rust (🦀 rust btw).

This is mostly a "for me" tool — built for my own workflow. Not saying you shouldn't use it, but don't expect it to be a community project. But hey, it works for me! The goal: everything lazygit does, but faster and with opinions I actually agree with.

**Why fork?** PRs were sitting too long, or the upstream direction didn't match how I wanted to work.

### What's different

- [x] **AI commit messages** — works with whatever agent you already use (claude, opencode, codex, or my minimal shim [modelcli](https://github.com/blankeos/modelcli))
- [x] **Side-by-side diffs** with syntax highlighting by default (shoutout lumen), no pager hacks needed
- [x] **Better diff navigation** — `]`/`[` for hunks, `jk⬆︎⬇︎` for line-by-line scrolling.

### Planned

- [ ] commits pane overhaul
  - [ ] Better graph view (enabled by default)
  - [ ] Filter by branch
  - [ ] Filter by commit message (handy if you prefix with ticket IDs)
- [ ] Command palette (OpenCode-style) — still figuring this one out

MIT

Feel free to fork and give it your own spin.
