mod checks;
mod export;
mod graph;
mod model;
mod outline;
mod tui;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use model::Paper;
use std::{fs, path::PathBuf};

/// Plan the outline and the alphabet of a mathematical paper.
#[derive(Parser)]
#[command(name = "lemma", version)]
struct Cli {
    /// Plan file
    #[arg(short, long, default_value = "lemma.toml", global = true)]
    file: PathBuf,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Subcommand)]
enum Cmd {
    /// Open the full-screen editor (default)
    Edit,
    /// Create an empty plan with the default alphabet
    Init {
        #[arg(long)]
        title: Option<String>,
    },
    /// Print the outline as Markdown
    Outline,
    /// Check the dependencies and the alphabet; exits with 1 on errors
    Check,
    /// Print the dependency graph
    Graph {
        #[arg(long, value_enum, default_value_t = GraphFormat::Mermaid)]
        format: GraphFormat,
    },
    /// Write a LaTeX skeleton (sections/*.tex and macros.tex) for the CAS template
    Latex {
        /// Output directory
        #[arg(long, default_value = "lemma-latex")]
        out: PathBuf,
        /// Overwrite existing files
        #[arg(long)]
        force: bool,
    },
}

#[derive(Clone, Copy, ValueEnum)]
enum GraphFormat {
    Mermaid,
    Dot,
}

fn load(cli: &Cli) -> Result<Paper> {
    if !cli.file.exists() {
        bail!(
            "{} not found: run `lemma init` or `lemma` to start a plan",
            cli.file.display()
        );
    }
    Paper::load(&cli.file)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.cmd.as_ref().unwrap_or(&Cmd::Edit) {
        Cmd::Edit => tui::run(&cli.file),
        Cmd::Init { title } => {
            if cli.file.exists() {
                bail!("{} already exists", cli.file.display());
            }
            let mut p = Paper::new();
            p.paper.title = title.clone().unwrap_or_default();
            p.save(&cli.file)?;
            println!("Created {}. Run `lemma` to start.", cli.file.display());
            Ok(())
        }
        Cmd::Outline => {
            print!("{}", export::markdown(&load(&cli)?));
            Ok(())
        }
        Cmd::Check => {
            let diags = checks::run(&load(&cli)?);
            for d in &diags {
                let tag = match d.level {
                    checks::Level::Error => "error",
                    checks::Level::Warning => "warning",
                    checks::Level::Info => "info",
                };
                println!("{tag}: {}", d.message);
            }
            if diags.is_empty() {
                println!("All good.");
            }
            if diags.iter().any(|d| d.level == checks::Level::Error) {
                std::process::exit(1);
            }
            Ok(())
        }
        Cmd::Graph { format } => {
            let p = load(&cli)?;
            print!(
                "{}",
                match format {
                    GraphFormat::Mermaid => export::mermaid(&p),
                    GraphFormat::Dot => export::dot(&p),
                }
            );
            Ok(())
        }
        Cmd::Latex { out, force } => {
            let p = load(&cli)?;
            let files = export::latex(&p).map_err(anyhow::Error::msg)?;
            let existing: Vec<String> = files
                .iter()
                .filter(|(path, _)| out.join(path).exists())
                .map(|(path, _)| path.clone())
                .collect();
            if !existing.is_empty() && !force {
                bail!(
                    "would overwrite {} in {} (use --force)",
                    existing.join(", "),
                    out.display()
                );
            }
            for (path, contents) in &files {
                let dest = out.join(path);
                if let Some(dir) = dest.parent() {
                    fs::create_dir_all(dir)?;
                }
                fs::write(&dest, contents)?;
                println!("wrote {}", dest.display());
            }
            println!("Add \\input{{macros}} and \\input{{sections/inputs}} to main.tex.");
            Ok(())
        }
    }
}
