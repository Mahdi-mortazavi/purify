//! `purify` command-line interface.

use std::path::{Path, PathBuf};

use anyhow::Context;
use clap::{Parser, Subcommand};
use purify_core::scan::Scanner;
use purify_core::{
    format_bytes, guardian, AnalysisReport, Analyzer, Confidence, FileEntry, ItemStatus,
    OrganizeLog, Organizer, QuarantineRequest, QuarantineStore, SignatureSet, SpaceInfo,
    Suggestion, Thresholds, UsageCollector, UsageReport, WalkScanner,
};
use purify_ntfs::MftScanner;
use tracing::info;
use tracing_subscriber::EnvFilter;

/// Default retention window (days) before a quarantined item may be purged.
const DEFAULT_RETENTION_DAYS: u32 = 30;

/// Confidence tiers selectable on the command line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum ConfidenceArg {
    /// Only items regenerated automatically with zero user impact.
    Safe,
    /// Safe plus near-certain items (e.g. old installers).
    LikelySafe,
    /// Everything, including items the user should review.
    ReviewNeeded,
}

impl ConfidenceArg {
    fn as_core(self) -> Confidence {
        match self {
            ConfidenceArg::Safe => Confidence::Safe,
            ConfidenceArg::LikelySafe => Confidence::LikelySafe,
            ConfidenceArg::ReviewNeeded => Confidence::ReviewNeeded,
        }
    }
}

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
    /// Read-only: this only *suggests*. Nothing is moved or deleted. Use
    /// `clean` to act on suggestions via reversible quarantine.
    Analyze(AnalyzeArgs),

    /// Clean a path by moving suggested items into reversible quarantine.
    ///
    /// DRY-RUN BY DEFAULT: without `--apply` it only prints the plan. Nothing
    /// is ever deleted — items move to quarantine and can be restored.
    Clean(CleanArgs),

    /// List items currently held in quarantine.
    List,

    /// Restore a quarantined item to its original location.
    Restore(RestoreArgs),

    /// Permanently purge quarantined items (expired ones, or a specific id).
    Purge(PurgeArgs),

    /// Organize loose files into typed folders (preview by default, undoable).
    ///
    /// DRY-RUN BY DEFAULT: shows the planned moves. `--apply` performs them and
    /// records an undo log; `--undo` reverses the last applied run.
    Organize(OrganizeArgs),

    /// Report disk-space pressure for a drive and recommend action.
    Guard(GuardArgs),
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

#[derive(Debug, clap::Args)]
struct CleanArgs {
    /// Path to clean.
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Actually move items to quarantine. Without this flag it is a dry run.
    #[arg(long)]
    apply: bool,

    /// Lowest confidence tier to act on (higher tiers are always included).
    #[arg(long, value_enum, default_value_t = ConfidenceArg::Safe)]
    min_confidence: ConfidenceArg,

    /// Load additional community signatures from this directory (*.toml).
    #[arg(long, value_name = "DIR")]
    signatures: Option<PathBuf>,
}

#[derive(Debug, clap::Args)]
struct RestoreArgs {
    /// The quarantine item id (see `purify list`).
    id: String,
}

#[derive(Debug, clap::Args)]
struct PurgeArgs {
    /// Purge a specific item id instead of expired ones.
    #[arg(long)]
    id: Option<String>,

    /// Purge items quarantined more than this many days ago.
    #[arg(long, default_value_t = DEFAULT_RETENTION_DAYS)]
    older_than: u32,

    /// Required confirmation — purging is permanent and irreversible.
    #[arg(long)]
    yes: bool,
}

#[derive(Debug, clap::Args)]
struct OrganizeArgs {
    /// Path to organize (e.g. a Downloads folder).
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Destination base for organized folders (default: <PATH>/Organized).
    #[arg(long, value_name = "DIR")]
    dest: Option<PathBuf>,

    /// Actually perform the moves. Without this flag it is a dry run.
    #[arg(long)]
    apply: bool,

    /// Reverse the most recent applied organize run.
    #[arg(long)]
    undo: bool,
}

#[derive(Debug, clap::Args)]
struct GuardArgs {
    /// Path whose volume to inspect (e.g. `C:\\`).
    #[arg(default_value = ".")]
    path: PathBuf,

    /// Warn at this used-percentage (0-100).
    #[arg(long)]
    warn_pct: Option<f64>,

    /// Flag critical at this used-percentage (0-100).
    #[arg(long)]
    critical_pct: Option<f64>,

    /// Emit the report as JSON.
    #[arg(long)]
    json: bool,
}

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_tracing(cli.verbose);

    match cli.command {
        Command::Scan(args) => run_scan(args),
        Command::Analyze(args) => run_analyze(args),
        Command::Clean(args) => run_clean(args),
        Command::List => run_list(),
        Command::Restore(args) => run_restore(args),
        Command::Purge(args) => run_purge(args),
        Command::Organize(args) => run_organize(args),
        Command::Guard(args) => run_guard(args),
    }
}

/// Current time as Unix seconds (0 if the clock is before the epoch).
fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/// Open the default per-user quarantine store (DB + blob area under the
/// platform data directory).
fn open_store() -> anyhow::Result<QuarantineStore> {
    let base = dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("purify");
    let db = base.join("quarantine.db");
    let blobs = base.join("quarantine");
    QuarantineStore::open(&db, &blobs).context("opening quarantine store")
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

fn run_clean(args: CleanArgs) -> anyhow::Result<()> {
    let root = std::path::absolute(&args.path).unwrap_or(args.path.clone());
    if !root.exists() {
        anyhow::bail!("clean target does not exist: {}", root.display());
    }

    let mut signatures = SignatureSet::builtin();
    if let Some(dir) = &args.signatures {
        let extra = SignatureSet::load_dir(dir)
            .with_context(|| format!("loading signatures from {}", dir.display()))?;
        signatures.signatures.extend(extra.signatures);
    }

    let mut entries: Vec<FileEntry> = Vec::new();
    WalkScanner::new()
        .scan(&root, &mut |e| entries.push(e))
        .with_context(|| format!("scanning {}", root.display()))?;

    let report = Analyzer::new(signatures).analyze(&entries, now_unix());
    let max_rank = args.min_confidence.as_core().rank();
    let selected: Vec<&Suggestion> = report
        .suggestions
        .iter()
        .filter(|s| label_rank(&s.confidence) <= max_rank)
        .collect();

    println!();
    println!("  purify clean — {}", root.display());
    if selected.is_empty() {
        println!(
            "  Nothing to clean at or below '{}'.",
            args.min_confidence.as_core().label()
        );
        println!();
        return Ok(());
    }

    let total: u64 = selected.iter().map(|s| s.size).sum();

    if !args.apply {
        println!("  DRY RUN — the following would move to quarantine:");
        println!();
        for s in &selected {
            println!(
                "    {:>10}  [{}]  {}",
                format_bytes(s.size),
                s.confidence,
                s.path.display()
            );
        }
        println!();
        println!(
            "  Would reclaim {} across {} items.",
            format_bytes(total),
            selected.len()
        );
        println!("  Re-run with --apply to move these to reversible quarantine.");
        println!();
        return Ok(());
    }

    let store = open_store()?;
    let now = now_unix();
    let mut moved = 0u64;
    let mut reclaimed = 0u64;
    for s in &selected {
        let req = QuarantineRequest {
            original_path: s.path.clone(),
            size: s.size,
            is_dir: s.is_dir,
            reason: s.reason.clone(),
            signature_id: Some(s.signature_id.clone()),
            confidence: Some(s.confidence.clone()),
        };
        match store.quarantine(&req, now) {
            Ok(item) => {
                moved += 1;
                reclaimed += s.size;
                println!("    quarantined {}  ({})", s.path.display(), item.id);
            }
            Err(err) => {
                tracing::warn!(path = %s.path.display(), %err, "skipping item");
                println!("    skipped {} — {}", s.path.display(), err);
            }
        }
    }
    println!();
    println!(
        "  Moved {moved} items ({}) to quarantine. Restore any with `purify restore <id>`.",
        format_bytes(reclaimed)
    );
    println!(
        "  They will remain restorable until purged (default retention {DEFAULT_RETENTION_DAYS} days)."
    );
    println!();
    Ok(())
}

fn run_list() -> anyhow::Result<()> {
    let store = open_store()?;
    let items = store.list(None).context("listing quarantine")?;
    let active: Vec<_> = items
        .iter()
        .filter(|i| i.status == ItemStatus::Quarantined)
        .collect();

    println!();
    if active.is_empty() {
        println!("  Quarantine is empty.");
        println!();
        return Ok(());
    }
    println!("  Quarantined items:");
    println!();
    let now = now_unix();
    for i in &active {
        let age_days = (now.saturating_sub(i.quarantined_at)) / 86_400;
        println!(
            "    {}  {:>10}  [{}]  {}",
            i.id,
            format_bytes(i.size),
            i.confidence.as_deref().unwrap_or("?"),
            i.original_path.display()
        );
        println!("      └─ quarantined {age_days}d ago — {}", i.reason);
    }
    println!();
    let total: u64 = active.iter().map(|i| i.size).sum();
    println!("  {} items, {} held.", active.len(), format_bytes(total));
    println!();
    Ok(())
}

fn run_restore(args: RestoreArgs) -> anyhow::Result<()> {
    let store = open_store()?;
    let item = store.get(&args.id).context("looking up item")?;
    store
        .restore(&args.id)
        .with_context(|| format!("restoring {}", args.id))?;
    println!("Restored {} to {}", args.id, item.original_path.display());
    Ok(())
}

fn run_purge(args: PurgeArgs) -> anyhow::Result<()> {
    let store = open_store()?;

    if let Some(id) = args.id {
        let item = store.get(&id).context("looking up item")?;
        if !args.yes {
            println!(
                "Would permanently delete {} ({}). Re-run with --yes to confirm.",
                id,
                item.original_path.display()
            );
            return Ok(());
        }
        store.purge(&id).with_context(|| format!("purging {id}"))?;
        println!("Purged {id} permanently.");
        return Ok(());
    }

    // Expired-purge mode.
    let now = now_unix();
    let cutoff = now.saturating_sub(i64::from(args.older_than) * 86_400);
    let candidates: Vec<_> = store
        .list(Some(ItemStatus::Quarantined))?
        .into_iter()
        .filter(|i| i.quarantined_at <= cutoff)
        .collect();

    if candidates.is_empty() {
        println!("No quarantined items older than {} days.", args.older_than);
        return Ok(());
    }
    if !args.yes {
        println!(
            "Would permanently purge {} item(s) older than {} days:",
            candidates.len(),
            args.older_than
        );
        for i in &candidates {
            println!("  {}  {}", i.id, i.original_path.display());
        }
        println!("Re-run with --yes to confirm.");
        return Ok(());
    }
    let purged = store.purge_expired(args.older_than, now)?;
    println!("Purged {} expired item(s) permanently.", purged.len());
    Ok(())
}

/// Path to the organize undo-log (records the most recent applied run).
fn organize_log_path() -> PathBuf {
    dirs::data_dir()
        .unwrap_or_else(std::env::temp_dir)
        .join("purify")
        .join("organize-log.json")
}

fn run_organize(args: OrganizeArgs) -> anyhow::Result<()> {
    if args.undo {
        let log_path = organize_log_path();
        let data = std::fs::read_to_string(&log_path)
            .with_context(|| format!("no organize log to undo at {}", log_path.display()))?;
        let log: OrganizeLog = serde_json::from_str(&data).context("parsing organize log")?;
        Organizer::undo(&log).context("undoing organize run")?;
        let _ = std::fs::remove_file(&log_path);
        println!("Reverted {} moved file(s).", log.moves.len());
        return Ok(());
    }

    let root = std::path::absolute(&args.path).unwrap_or(args.path.clone());
    if !root.exists() {
        anyhow::bail!("organize target does not exist: {}", root.display());
    }
    let dest = args.dest.unwrap_or_else(|| root.join("Organized"));

    let mut entries: Vec<FileEntry> = Vec::new();
    WalkScanner::new()
        .scan(&root, &mut |e| entries.push(e))
        .with_context(|| format!("scanning {}", root.display()))?;

    let organizer = Organizer::with_defaults();
    let plan = organizer.plan(&entries, &dest, now_unix());

    println!();
    println!(
        "  purify organize — {} → {}",
        root.display(),
        dest.display()
    );
    if plan.moves.is_empty() {
        println!("  Nothing to organize (no matching files old enough).");
        println!();
        return Ok(());
    }

    if !args.apply {
        println!("  DRY RUN — planned moves:");
        println!();
        for m in &plan.moves {
            println!("    {}  →  {}", m.from.display(), m.to.display());
        }
        println!();
        println!(
            "  {} file(s) would be organized. Re-run with --apply.",
            plan.moves.len()
        );
        println!();
        return Ok(());
    }

    let log = Organizer::apply(&plan).context("applying organize plan")?;
    let log_path = organize_log_path();
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent).ok();
    }
    std::fs::write(&log_path, serde_json::to_vec_pretty(&log)?)
        .with_context(|| format!("writing organize log to {}", log_path.display()))?;
    println!(
        "  Organized {} file(s). Undo with `purify organize --undo`.",
        log.moves.len()
    );
    println!();
    Ok(())
}

fn run_guard(args: GuardArgs) -> anyhow::Result<()> {
    let root = std::path::absolute(&args.path).unwrap_or(args.path.clone());
    let total = fs2::total_space(&root)
        .with_context(|| format!("reading total space for {}", root.display()))?;
    let available = fs2::available_space(&root)
        .with_context(|| format!("reading free space for {}", root.display()))?;
    let space = SpaceInfo { total, available };

    let mut thresholds = Thresholds::default();
    if let Some(w) = args.warn_pct {
        thresholds.warn = w / 100.0;
    }
    if let Some(c) = args.critical_pct {
        thresholds.critical = c / 100.0;
    }

    let report = guardian::evaluate(space, thresholds);

    if args.json {
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    let level = match report.pressure {
        purify_core::Pressure::Ok => "OK",
        purify_core::Pressure::Warning => "WARNING",
        purify_core::Pressure::Critical => "CRITICAL",
    };
    println!();
    println!("  purify guard — {}", root.display());
    println!(
        "  {} used of {}  ({} free) — {:.0}% used  [{level}]",
        format_bytes(space.used()),
        format_bytes(space.total),
        format_bytes(space.available),
        report.used_fraction * 100.0,
    );
    println!("  {}", report.recommendation);
    println!();
    Ok(())
}

/// Rank a confidence label (lower = safer).
fn label_rank(label: &str) -> u8 {
    match label {
        "safe" => 0,
        "likely-safe" => 1,
        _ => 2,
    }
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
