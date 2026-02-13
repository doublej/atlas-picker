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
    #[allow(dead_code)]
    #[arg(short, long, default_value = "path")]
    output: String,

    /// Action to perform: print (default), cd, code, or run
    #[allow(dead_code)]
    #[arg(short, long, default_value = "print")]
    action: String,

    /// Color theme: auto (detect from terminal), light, or dark
    #[arg(short, long, default_value = "auto")]
    theme: String,

    /// Project Index API URL (defaults to env PROJECT_INDEX_API or http://localhost:47891)
    #[arg(long)]
    api_url: Option<String>,
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

    let api_url = args
        .api_url
        .or_else(|| std::env::var("PROJECT_INDEX_API").ok())
        .unwrap_or_else(|| "http://localhost:47891".to_string());

    let theme = theme::resolve(&args.theme);
    let selected = ui::run_picker(
        &cache.projects,
        theme,
        cache.scanned_at.as_deref(),
        base_dir.clone(),
        api_url.clone(),
    )?;

    if let Some(selection) = selected {
        let project = selection.project;
        use ui::{AgentAction, CodeAction, CopyAction, OpenAction, RunAction, SelectionAction};

        match selection.action {
            SelectionAction::Cd => {
                println!("{}", project.path);
            }
            SelectionAction::Open(action) => match action {
                OpenAction::Iterm => {
                    if let Err(e) = project::api_open_iterm(&api_url, &project.path) {
                        eprintln!("Failed to open iTerm: {e}");
                    }
                }
                OpenAction::Finder => {
                    std::process::Command::new("open")
                        .arg("-R")
                        .arg(&project.path)
                        .spawn()
                        .context("Failed to open Finder")?;
                }
                OpenAction::Open => {
                    std::process::Command::new("open")
                        .arg(&project.path)
                        .spawn()
                        .context("Failed to open")?;
                }
            },
            SelectionAction::Code(action) => match action {
                CodeAction::Claude => {
                    std::process::Command::new("claude")
                        .arg(&project.path)
                        .spawn()
                        .context("Failed to open Claude")?;
                }
                CodeAction::Codex => {
                    std::process::Command::new("codex")
                        .arg(&project.path)
                        .spawn()
                        .context("Failed to open Codex")?;
                }
                CodeAction::Opencode => {
                    std::process::Command::new("opencode")
                        .arg(&project.path)
                        .spawn()
                        .context("Failed to open Opencode")?;
                }
            },
            SelectionAction::Run(RunAction::Dev) => {
                if let Some(cmd) = &project.dev_command {
                    let runner = project.runner.as_deref();
                    if let Err(e) = project::api_run_dev(&api_url, &project.path, cmd, runner) {
                        eprintln!("Failed to run dev command: {e}");
                    }
                } else {
                    eprintln!("No dev command found for {}", project.name);
                }
            }
            SelectionAction::Agent(action) => match action {
                AgentAction::Claude => {
                    if let Err(e) =
                        project::api_agent_create(&api_url, &project.path, "claude", true)
                    {
                        eprintln!("Failed to open CLAUDE.md: {e}");
                    }
                }
                AgentAction::Agents => {
                    if let Err(e) =
                        project::api_agent_create(&api_url, &project.path, "agents", true)
                    {
                        eprintln!("Failed to open AGENTS.md: {e}");
                    }
                }
                AgentAction::CopyClaudeToAgents => {
                    if let Err(e) =
                        project::api_agent_copy(&api_url, &project.path, "claude", "agents")
                    {
                        eprintln!("Failed to copy CLAUDE.md to AGENTS.md: {e}");
                    }
                }
                AgentAction::CopyAgentsToClaude => {
                    if let Err(e) =
                        project::api_agent_copy(&api_url, &project.path, "agents", "claude")
                    {
                        eprintln!("Failed to copy AGENTS.md to CLAUDE.md: {e}");
                    }
                }
            },
            SelectionAction::Copy(action) => match action {
                CopyAction::Path => {
                    use arboard::Clipboard;
                    if let Ok(mut clipboard) = Clipboard::new() {
                        if clipboard.set_text(&project.path).is_ok() {
                            eprintln!("Path copied to clipboard");
                        } else {
                            eprintln!("Failed to copy to clipboard");
                        }
                    } else {
                        eprintln!("Failed to access clipboard");
                    }
                }
                CopyAction::DevCommand => {
                    if let Some(cmd) = &project.dev_command {
                        let runner_cmd = match project.runner.as_deref() {
                            Some("bun") => "bun".to_string(),
                            Some("uv") => "uv run".to_string(),
                            Some(runner) => format!("{runner} run"),
                            None => "npm run".to_string(),
                        };
                        let full = format!("cd {} && {} {}", project.path, runner_cmd, cmd);
                        use arboard::Clipboard;
                        if let Ok(mut clipboard) = Clipboard::new() {
                            if clipboard.set_text(&full).is_ok() {
                                eprintln!("Dev command copied to clipboard");
                            } else {
                                eprintln!("Failed to copy to clipboard");
                            }
                        } else {
                            eprintln!("Failed to access clipboard");
                        }
                    } else {
                        eprintln!("No dev command found for {}", project.name);
                    }
                }
            },
            SelectionAction::Deploy(sel) => {
                std::process::Command::new("open")
                    .arg(&sel.url)
                    .spawn()
                    .context("Failed to open deploy URL")?;
            }
        }
    }

    Ok(())
}
