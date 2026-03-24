mod app;
mod config;
mod git;
mod gui;
mod model;
mod os;

use std::path::PathBuf;

use clap::Parser;

#[derive(Parser)]
#[command(name = "lazygitrs", version, about = "A fast terminal UI for git")]
struct Cli {
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
}

fn main() {
    let cli = Cli::parse();

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

    match app::App::new(repo_path, cli.debug) {
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
