mod project;
mod splash;
mod theme;
mod ui;

use anyhow::{Context, Result};
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "project-picker", about = "Fast fuzzy project picker")]
struct Args {
    /// Base directory to search (uses ~/Documents/development if not specified)
    #[arg(short, long)]
    dir: Option<PathBuf>,

    /// Output format: path (default), json, or name
    #[arg(short, long, default_value = "path")]
    output: String,

    /// Action to perform: print (default), cd, code, or run
    #[arg(short, long, default_value = "print")]
    action: String,

    /// Color theme: auto (detect from terminal), light, or dark
    #[arg(short, long, default_value = "auto")]
    theme: String,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let base_dir = args.dir.unwrap_or_else(|| {
        dirs::home_dir()
            .map(|h| h.join("Documents/development"))
            .expect("Could not determine home directory")
    });

    let cache_path = base_dir.join(".project-index-cache.json");

    let cache = project::load_cache(&cache_path)
        .with_context(|| format!("Failed to load cache from {}", cache_path.display()))?;

    if cache.projects.is_empty() {
        eprintln!("No projects found in cache. Run project-index first to scan.");
        std::process::exit(1);
    }

    let theme = theme::resolve(&args.theme);
    let selected = ui::run_picker(&cache.projects, theme, cache.scanned_at.as_deref())?;

    if let Some(project) = selected {
        match args.action.as_str() {
            "print" => match args.output.as_str() {
                "json" => println!("{}", serde_json::to_string(&project)?),
                "name" => println!("{}", project.name),
                _ => println!("{}", project.path),
            },
            "cd" => {
                // Output for shell eval: cd "/path/to/project"
                println!("cd {:?}", project.path);
            }
            "code" => {
                std::process::Command::new("code")
                    .arg(&project.path)
                    .spawn()
                    .context("Failed to open VS Code")?;
            }
            "run" => {
                if let Some(cmd) = &project.dev_command {
                    let runner = project.runner.as_deref().unwrap_or("npm");
                    println!("cd {:?} && {} run {}", project.path, runner, cmd);
                } else {
                    eprintln!("No dev command found for {}", project.name);
                    std::process::exit(1);
                }
            }
            _ => println!("{}", project.path),
        }
    }

    Ok(())
}
