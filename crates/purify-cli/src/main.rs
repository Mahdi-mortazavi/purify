//! `purify` command-line interface.
//!
//! Phase 0 wires up argument parsing, structured logging, and the command
//! surface so the binary is usable and testable end to end. The `scan` command
//! reports that real scanning arrives in Phase 1 rather than doing anything to a
//! user's disk — consistent with purify's "safety over speed of development"
//! and "dry-run by default" principles.

use std::path::PathBuf;

use anyhow::Context;
use clap::{Parser, Subcommand};
use purify_ntfs::MftScanner;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Fast, safe, reversible disk cleanup and organization for Windows.
#[derive(Debug, Parser)]
#[command(name = "purify", version, about, long_about = None)]
struct Cli {
    /// Increase log verbosity (can be passed multiple times: -v, -vv).
    #[arg(short, long, action = clap::ArgAction::Count, global = true)]
    verbose: u8,

    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Scan a drive or directory and report its largest space consumers.
    ///
    /// Uses direct NTFS MFT reads when available (Windows + admin), otherwise
    /// falls back to a portable directory walk. Read-only: never modifies disk.
    Scan {
        /// Path to scan (e.g. `C:\\` on Windows, or any directory).
        #[arg(default_value = ".")]
        path: PathBuf,

        /// How many top consumers to display.
        #[arg(short, long, default_value_t = 20)]
        top: usize,
    },
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::Scan { path, top } => run_scan(&path, top),
    }
}

/// Configure structured logging. Verbosity flags raise the level; `RUST_LOG`
/// still overrides when set, so power users keep full control.
fn init_tracing(verbose: u8) {
    let default_level = match verbose {
        0 => "info",
        1 => "debug",
        _ => "trace",
    };
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(format!("purify={default_level},warn")));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

fn run_scan(path: &std::path::Path, top: usize) -> anyhow::Result<()> {
    let path = path
        .canonicalize()
        .with_context(|| format!("cannot access scan target: {}", path.display()))?;

    info!(target = %path.display(), top, "scan requested");

    if MftScanner::is_available() {
        println!("Direct MFT scanning is available on this platform.");
    } else {
        println!("Direct MFT scanning is unavailable here; Phase 1 will use the portable walker.");
    }
    println!(
        "Real scanning of '{}' (top {top}) lands in Phase 1. No files were touched.",
        path.display()
    );

    Ok(())
}
