mod app;
mod config;
mod git;
mod gui;
mod model;
mod os;
mod pager;
mod upgrade;

use std::path::PathBuf;

use clap::{Parser, Subcommand};

const LOGO: &str = include_str!("../logo.txt");

#[derive(Parser)]
#[command(name = "lazygitrs", version, about = "A fast and ergonomic terminal UI for git", before_help = LOGO)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,

    /// Path to the git repository
    #[arg(short, long)]
    path: Option<PathBuf>,

    /// Git work tree path
    #[arg(short = 'w', long = "work-tree")]
    work_tree: Option<PathBuf>,

    /// Git dir path
    #[arg(short = 'g', long = "git-dir")]
    git_dir: Option<PathBuf>,

    /// Enable debug logging
    #[arg(short, long)]
    debug: bool,

    /// Filter commits by path (file or directory), like lazygit -f
    #[arg(short = 'f', long = "filter", value_name = "PATH")]
    filter_path: Option<PathBuf>,

    /// Print the default configuration YAML to stdout and exit
    #[arg(long)]
    print_default_config: bool,

    /// Configuration file or preset name to use (e.g. 'popup' or ~/.config/lazygitrs/config.yml)
    #[arg(short = 'c', long = "config")]
    config: Option<String>,

    /// Launch directly in the commits panel
    #[arg(long)]
    commits: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Upgrade lazygitrs to the latest (or a specific) version
    Upgrade {
        /// Target version (e.g. `0.0.32`) or `latest`
        target: Option<String>,
    },
}

/// Restore the terminal on panic so the user isn't left in raw mode + mouse
/// capture (which makes the shell unusable — every mouse move spews escape
/// sequences into the prompt).
fn install_panic_hook() {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        // Prefer /dev/tty when stdout is redirected (Helix `:insert-output`).
        let mut out =
            crate::os::tty::open_tui_output().unwrap_or_else(|_| Box::new(std::io::stdout()));
        if crate::os::tty::nested_tty_launch() {
            // Same contract as restore_terminal: Helix still owns alt-screen /
            // raw / mouse. Only undo our kitty push and hand the tty back.
            let _ = crossterm::execute!(out, crossterm::cursor::Show);
            let _ = crossterm::execute!(
                out,
                crossterm::event::PopKeyboardEnhancementFlags,
                crossterm::event::PushKeyboardEnhancementFlags(
                    crossterm::event::KeyboardEnhancementFlags::DISAMBIGUATE_ESCAPE_CODES
                        | crossterm::event::KeyboardEnhancementFlags::REPORT_ALTERNATE_KEYS
                ),
            );
            crate::os::tty::restore_foreground_tty();
        } else {
            crate::os::tty::restore_foreground_tty();
            let _ = crossterm::execute!(
                out,
                crossterm::event::DisableMouseCapture,
                crossterm::event::DisableFocusChange,
                crossterm::cursor::Show,
                crossterm::terminal::LeaveAlternateScreen,
            );
            let _ = crossterm::terminal::disable_raw_mode();
        }
        prev(info);
    }));
}

fn main() {
    // Helix `:insert-output` sets stdin=/dev/null + stdout=pipe while keeping its
    // EventStream on /dev/tty. Detect that, claim the tty foreground so Helix
    // can't steal keys, and draw on a separate /dev/tty handle (no stdout dup2).
    os::tty::reclaim_controlling_tty();
    install_panic_hook();
    let cli = Cli::parse();

    if let Some(Commands::Upgrade { target }) = cli.command {
        if let Err(e) = upgrade::upgrade(target.as_deref()) {
            eprintln!("Error: {e:#}");
            std::process::exit(1);
        }
        return;
    }

    if cli.print_default_config {
        let config = config::user_config::UserConfig::default();
        let yaml = serde_yaml::to_string(&config).unwrap();
        println!("{}", yaml);
        std::process::exit(0);
    }

    // Set up logging if debug mode
    if cli.debug {
        tracing_subscriber::fmt()
            .with_env_filter("lazygitrs=debug")
            .with_writer(std::io::stderr)
            .init();
    }

    let repo_path = cli
        .path
        .or(cli.work_tree)
        .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")));

    match app::App::new(
        repo_path,
        cli.debug,
        cli.config,
        cli.filter_path,
        cli.commits,
    ) {
        Ok(app) => {
            if let Err(e) = app.run() {
                eprintln!("Error: {:#}", e);
                std::process::exit(1);
            }
        }
        Err(e) => {
            eprintln!("Error: {:#}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_commits_flag() {
        let cli = Cli::parse_from(["lazygitrs", "--commits"]);
        assert!(cli.commits);

        let cli_default = Cli::parse_from(["lazygitrs"]);
        assert!(!cli_default.commits);
    }

    #[test]
    fn test_cli_config_flag() {
        let cli = Cli::parse_from(["lazygitrs", "-c", "my-config.yml"]);
        assert_eq!(cli.config.as_deref(), Some("my-config.yml"));

        let cli_long = Cli::parse_from(["lazygitrs", "--config", "popup"]);
        assert_eq!(cli_long.config.as_deref(), Some("popup"));

        let cli_default = Cli::parse_from(["lazygitrs"]);
        assert_eq!(cli_default.config, None);
    }

    #[test]
    fn test_cli_print_default_config_flag() {
        let cli = Cli::parse_from(["lazygitrs", "--print-default-config"]);
        assert!(cli.print_default_config);
    }
}
