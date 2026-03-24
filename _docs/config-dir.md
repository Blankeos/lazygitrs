# Config Directory

lazygitrs reads its configuration from `~/.config/lazygit/config.yml`, matching the original lazygit's behavior.

## Why not `dirs::config_dir()`?

The Rust `dirs` crate returns platform-specific paths:
- **macOS**: `~/Library/Application Support/`
- **Linux**: `~/.config/` (or `$XDG_CONFIG_HOME`)

The original lazygit (Go) always uses `~/.config/lazygit/` on all platforms. Since lazygitrs aims to be a drop-in replacement and share the same config file, we use `~/.config/lazygit/` as well.

## XDG_CONFIG_HOME

If the `XDG_CONFIG_HOME` environment variable is set, lazygitrs uses `$XDG_CONFIG_HOME/lazygit/` instead. This matches lazygit's behavior.

## Implementation

See `src/config/mod.rs` — the config directory is resolved as:

```
$XDG_CONFIG_HOME/lazygit/   (if XDG_CONFIG_HOME is set)
~/.config/lazygit/           (otherwise)
```
