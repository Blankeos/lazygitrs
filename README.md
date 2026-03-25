# lazygitrs

A faster, memory-safe, more ergonomic slopfork of lazygit (🦀 rust btw).

This is mostly a "for me" tool — built for my own workflow. Not saying you shouldn't use it, but don't expect it to be a community project. But hey, it works for me!

**Why fork?** PRs were sitting too long, or the upstream direction didn't match how I wanted to work.

The goal: everything lazygit does, but faster and with opinions I actually agree with. (I can't promise backwards-compat w/ lazygit's config since it'll eventually drift w/ my own opinions, but I made sure to do that)

### Install

```sh
npm install -g @blankeos/lazygitrs  # npm
bun install -g @blankeos/lazygitrs  # or bun
cargo binstall lazygitrs            # or cargo-binstall (prebuilt binary, faster)
cargo install lazygitrs             # or cargo (build from source)
```

Then run:

```sh
lazygitrs
```

### What's different

- [x] **AI commit messages** — works with whatever agent you already use (claude, opencode, codex, or my minimal shim [modelcli](https://github.com/blankeos/modelcli))
      Configure it in `~/.config/lazygit/config.toml`:

  ```toml
  # ~/.config/lazygit/config.toml
  git:
    commit:
      # Using claude
      generateCommand: "claude -p 'Generate a conventional commit message for this diff.'"
      # Using opencode
      generateCommand: "opencode run 'Generate a conventional commit message for this diff.'"
      # Using modelcli
      generateCommand: 'DIFF=$(git diff --cached) && modelcli "Generate a conventional commit message for this diff. Always provide a bulletpoint body. $DIFF"'
  ```

- [x] **Side-by-side diffs** with syntax highlighting by default, no pager hacks needed
- [x] **Better diff navigation UX** — `[]` new/old only views, `{}` for hunk traveling, `hjkl←↑↓→` for line-by-line scrolling, mouse highlights so you can copy. Lots inspired by [lumen](https://github.com/jnsahaj/lumen)
- [x] **Default GitHub conveniences** — copy repo url, open repo url, copy PR create url, open PR create, copy pr url, open pr. (The 'copy' variants are useful if you use different default browsers for work/personal.)
- [x] **Branch Filtering** — better experience in the Commits tab, compare what actually matters.
- [ ] **Future: Built-in compare tool** — Again, inspired by lumen, but more built into the TUI. Pick a commit/branch A and a commit/branch B, then see how they differ.

MIT

Feel free to fork and give it your own spin.
