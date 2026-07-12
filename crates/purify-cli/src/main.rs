//! `purify` command-line interface.

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};
use purify_core::scan::Scanner;
use purify_core::{
    format_bytes, AnalysisReport, Analyzer, FileEntry, SignatureSet, UsageCollector, UsageReport,
    WalkScanner,
};
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
    /// Read-only: never modifies disk. Uses direct NTFS MFT reads when
    /// available (Windows + admin) and requested, otherwise a portable walk.
    Scan(ScanArgs),

    /// Analyze a path and suggest safe-to-reclaim files with confidence levels.
    ///
    /// Read-only: this only *suggests*. Nothing is moved or deleted. Use the
    /// (upcoming) `clean` command to act on suggestions via reversible
    /// quarantine.
    Analyze(AnalyzeArgs),
}

#[derive(Debug, clap::Args)]
struct ScanArgs {
    /// Path to scan (e.g. `C:\\` on Windows, or any directory).
    #[arg(default_value = ".")]
    path: PathBuf,

    /// How many top consumers to display.
    #[arg(short, long, default_value_t = 20)]
    top: usize,

    /// Prefer the direct NTFS MFT scanner (Windows only; needs admin). Falls
    /// back to the portable walker automatically if unavailable.
    #[arg(long)]
    mft: bool,

    /// Emit the report as JSON instead of a human-readable table.
    #[arg(long)]
    json: bool,
}

#[derive(Debug, clap::Args)]
struct AnalyzeArgs {
    /// Path to analyze (e.g. `C:\\Users\\me` or a Downloads folder).
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Emit suggestions as JSON instead of a human-readable table.
    #[arg(long)]
    json: bool,

    /// Load additional community signatures from this directory (*.toml).
    #[arg(long, value_name = "DIR")]
    signatures: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::Scan(args) => run_scan(args),
        Command::Analyze(args) => run_analyze(args),
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

fn run_scan(args: ScanArgs) -> anyhow::Result<()> {
    let root = std::path::absolute(&args.path).unwrap_or(args.path.clone());
    if !root.exists() {
        anyhow::bail!("scan target does not exist: {}", root.display());
    }

    let (strategy, report) = scan_with_best_strategy(&root, args.top, args.mft)
        .with_context(|| format!("scanning {}", root.display()))?;

    if args.json {
        let json = serde_json::to_string_pretty(&report).context("serializing report")?;
        println!("{json}");
    } else {
        print_report(&report, strategy);
    }
    Ok(())
}

/// Run the scan, preferring the MFT scanner when requested and available, and
/// transparently falling back to the portable walker.
fn scan_with_best_strategy(
    root: &Path,
    top: usize,
    prefer_mft: bool,
) -> anyhow::Result<(&'static str, UsageReport)> {
    if prefer_mft && MftScanner::is_available() {
        match collect(root, top, &MftScanner::new()) {
            Ok(report) => return Ok((MftScanner::new().strategy_name(), report)),
            Err(err) => {
                tracing::warn!(%err, "MFT scan failed; falling back to portable walker");
            }
        }
    } else if prefer_mft {
        info!("MFT scanning unavailable on this platform; using portable walker");
    }

    let walker = WalkScanner::new();
    let report = collect(root, top, &walker)?;
    Ok((walker.strategy_name(), report))
}

fn collect(root: &Path, top: usize, scanner: &dyn Scanner) -> anyhow::Result<UsageReport> {
    let mut collector = UsageCollector::new(root);
    scanner.scan(root, &mut |entry| collector.record(&entry))?;
    Ok(collector.into_report(top))
}

fn run_analyze(args: AnalyzeArgs) -> anyhow::Result<()> {
    let root = std::path::absolute(&args.path).unwrap_or(args.path.clone());
    if !root.exists() {
        anyhow::bail!("analyze target does not exist: {}", root.display());
    }

    // Load signatures: the embedded defaults plus any user-supplied directory.
    let mut signatures = SignatureSet::builtin();
    if let Some(dir) = &args.signatures {
        let extra = SignatureSet::load_dir(dir)
            .with_context(|| format!("loading signatures from {}", dir.display()))?;
        info!(count = extra.len(), "loaded extra signatures");
        signatures.signatures.extend(extra.signatures);
    }

    // Collect the full entry list (analyze needs it for directory sizing and
    // age-based rules). The portable walker provides modification times.
    let mut entries: Vec<FileEntry> = Vec::new();
    WalkScanner::new()
        .scan(&root, &mut |e| entries.push(e))
        .with_context(|| format!("scanning {}", root.display()))?;

    let now_unix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let report = Analyzer::new(signatures).analyze(&entries, now_unix);

    if args.json {
        let json = serde_json::to_string_pretty(&report).context("serializing analysis")?;
        println!("{json}");
    } else {
        print_analysis(&report, &root);
    }
    Ok(())
}

fn print_analysis(report: &AnalysisReport, root: &Path) {
    use purify_core::Confidence;
    println!();
    println!("  purify analyze — {}", root.display());
    println!();
    if report.suggestions.is_empty() {
        println!("  No reclaimable items matched. Your drive looks tidy here.");
        println!();
        return;
    }

    for s in &report.suggestions {
        let kind = if s.is_dir { "dir " } else { "file" };
        println!(
            "  [{:>13}] {:>10}  {} ({})",
            s.confidence,
            format_bytes(s.size),
            s.path.display(),
            kind
        );
        println!("                 └─ {} — {}", s.title, s.reason);
    }
    println!();
    println!(
        "  Reclaimable — safe: {}   ≤ likely-safe: {}   total: {}",
        format_bytes(report.reclaimable_up_to(Confidence::Safe)),
        format_bytes(report.reclaimable_up_to(Confidence::LikelySafe)),
        format_bytes(report.total_reclaimable),
    );
    println!("  (read-only: nothing was moved or deleted)");
    println!();
}

fn print_report(report: &UsageReport, strategy: &str) {
    println!();
    println!("  purify scan — {}", report.root.display());
    println!("  strategy: {strategy}");
    println!(
        "  total: {}  across {} files",
        format_bytes(report.total_bytes),
        report.file_count
    );
    println!();
    if report.top.is_empty() {
        println!("  (nothing found)");
        return;
    }
    println!("  Largest consumers:");
    let width = report
        .top
        .iter()
        .map(|c| format_bytes(c.size).len())
        .max()
        .unwrap_or(0);
    for (i, c) in report.top.iter().enumerate() {
        let kind = if c.is_dir { "dir " } else { "file" };
        let name = c
            .path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| c.path.display().to_string());
        println!(
            "  {:>2}. {:>width$}  [{kind}]  {name}",
            i + 1,
            format_bytes(c.size),
            width = width
        );
    }
    println!();
}
