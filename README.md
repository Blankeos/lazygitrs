# lazygitrs

A slopfork/port of lazygit into rust.

It's literally just a "something I would use" tool. I don't recommend everyone else using it. Aims to do everything lazygit can, but better + faster (🦀 rust).
I just forked because I wanted features that were either to slow to merge or differs w/ the original authors opinions:

- [x] generating ai commit messages w/ your existing agent (claude, opencode, codex, or my tool minimal tool for it [modelcli](https://github.com/blankeos/modelcli))
- [x] side-by-side diffing w/ syntax highlighting by default (inspired by lumen), no need to add a separate pager.
- [ ] future: better ergonomics for the [4] commits bar
  - [ ] Better graph view (on by default)
  - [ ] Filter by branch, etc.
  - [ ] Filter by commit message useful if you conventionally use ticket names etc.
- [ ] future: command palette (like OpenCode) - not sure what to do yet for this tho.

License MIT
