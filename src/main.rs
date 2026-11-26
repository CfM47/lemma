mod checks;
mod export;
mod graph;
mod model;
mod outline;
mod sync;
mod tex;
mod tui;

use anyhow::{Result, bail};
use clap::{Parser, Subcommand, ValueEnum};
use model::Paper;
use std::path::PathBuf;

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
    /// Write the LaTeX skeleton for the CAS template, or add what is missing to an existing one
    ///
    /// Statements of the plan that have no \label in the sources get a stub in their
    /// section, next to their neighbours; text already written is never touched.
    Latex {
        /// Directory of the paper
        #[arg(long, default_value = "lemma-latex")]
        out: PathBuf,
        /// Rewrite every generated file from scratch
        #[arg(long)]
        force: bool,
    },
    /// Read statements, labels and references from LaTeX sources into the plan
    ///
    /// A directory is read from its main.tex (or files with \documentclass), following \input.
    /// Theorem-like environments with a \label become statements; \ref in a statement or
    /// its proof becomes a dependency. Existing statements are kept.
    Import {
        /// .tex files or directories
        #[arg(required = true)]
        paths: Vec<PathBuf>,
        /// Show what would change without saving
        #[arg(long)]
        dry_run: bool,
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
            let rep = sync::sync(&p, out, *force)?;
            for f in &rep.created {
                println!("created {}", f.display());
            }
            for f in &rep.changed {
                println!("updated {}", f.display());
            }
            for i in &rep.inserted {
                println!("  + {i}");
            }
            for n in &rep.notes {
                println!("note: {n}");
            }
            if rep.created.is_empty() && rep.changed.is_empty() {
                println!("{} is up to date.", out.display());
            }
            Ok(())
        }
        Cmd::Import { paths, dry_run } => {
            let mut p = if cli.file.exists() {
                Paper::load(&cli.file)?
            } else {
                Paper::new()
            };
            let doc = tex::TexDoc::read(paths)?;
            let rep = sync::import(&mut p, &doc);
            println!("read {} file(s)", rep.files);
            if let Some(t) = &rep.title {
                println!("title: {t}");
            }
            for a in &rep.added {
                println!("  + {}", p.label(a));
            }
            println!(
                "{} statement(s) added, {} dependenc(ies) added",
                rep.added.len(),
                rep.links
            );
            if !rep.mains.is_empty() {
                let names: Vec<String> = rep.mains.iter().map(|m| p.label(m)).collect();
                println!(
                    "main results guessed (theorems only corollaries, remarks or examples use; change with m in the editor): {}",
                    names.join(", ")
                );
            }
            if rep.unlabeled > 0 {
                println!(
                    "note: {} environment(s) without \\label were skipped",
                    rep.unlabeled
                );
            }
            for r in &rep.refused {
                println!("note: dependency skipped: {r}");
            }
            if *dry_run {
                println!("dry run: {} not written", cli.file.display());
            } else {
                p.save(&cli.file)?;
                println!("saved {}", cli.file.display());
            }
            Ok(())
        }
    }
}
