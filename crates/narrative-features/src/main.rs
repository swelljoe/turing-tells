use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};

use narrative_features::{Annotation, Backend, Sink, agree, features, load, malformed, remaining, report};

#[derive(Parser)]
#[command(
    name = "narrative-features",
    about = "Annotate prose with StoryScope's 30 core narrative features and measure rater reliability"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Assemble story-scale passages by joining consecutive corpus chunks.
    Sample {
        #[arg(long, default_value = "corpus-work/chunks")]
        chunks: PathBuf,
        #[arg(long, default_value = "corpus-work/passages")]
        out: PathBuf,
        /// Chunks per passage; ~2k words each, so 3 gives story scale.
        #[arg(long, default_value_t = 3)]
        join: usize,
        /// Passages per book, taken from the middle where the story is running.
        #[arg(long, default_value_t = 1)]
        per_book: usize,
        #[arg(long)]
        limit: Option<usize>,
    },
    /// Ask a rater the 30 questions about every passage.
    Annotate {
        #[arg(long, default_value = "corpus-work/passages")]
        passages: PathBuf,
        /// Rater identity, e.g. "gemma-4-31b". Runs pool under this name.
        #[arg(long)]
        rater: String,
        /// llama-server base URL; repeat to spread load across machines.
        #[arg(long = "endpoint")]
        endpoints: Vec<String>,
        /// Shell command reading a prompt on stdin, e.g. "claude -p".
        /// Repeat to run several in parallel.
        #[arg(long = "command")]
        commands: Vec<String>,
        #[arg(long, default_value_t = 1)]
        runs: usize,
        #[arg(long, default_value = "data/features.jsonl")]
        out: PathBuf,
        /// Give up if this many consecutive calls fail outright.
        #[arg(long, default_value_t = 8)]
        abort_after: usize,
    },
    /// Agreement per feature. Without --within, compares raters to each other.
    Report {
        #[arg(long, default_value = "data/features.jsonl")]
        input: PathBuf,
        /// Measure one rater's self-consistency across its own runs.
        #[arg(long)]
        within: Option<String>,
        /// Restrict to these raters (repeatable).
        #[arg(long = "rater")]
        raters: Vec<String>,
        /// Compare raters using only their first run, so a model is not
        /// rewarded for having been sampled more times than another.
        #[arg(long)]
        first_run_only: bool,
        /// Pool each rater's runs into one consensus answer before comparing,
        /// to see whether repeated sampling rescues a noisy rater.
        #[arg(long, conflicts_with_all = ["first_run_only", "within"])]
        aggregate: bool,
    },
}

/// Chunk files sort lexically within a book, so consecutive names are
/// consecutive prose.
fn books(chunks: &Path) -> Result<Vec<(String, Vec<PathBuf>)>> {
    let mut out = Vec::new();
    for author in fs::read_dir(chunks).with_context(|| format!("reading {}", chunks.display()))? {
        let author = author?.path();
        if !author.is_dir() {
            continue;
        }
        for book in fs::read_dir(&author)? {
            let book = book?.path();
            if !book.is_dir() {
                continue;
            }
            let mut files: Vec<PathBuf> = fs::read_dir(&book)?
                .filter_map(|e| e.ok().map(|e| e.path()))
                .filter(|p| p.extension().is_some_and(|e| e == "txt"))
                .collect();
            files.sort();
            if files.is_empty() {
                continue;
            }
            let name = format!(
                "{}/{}",
                author.file_name().unwrap().to_string_lossy(),
                book.file_name().unwrap().to_string_lossy()
            );
            out.push((name, files));
        }
    }
    out.sort();
    Ok(out)
}

fn sample(chunks: &Path, out: &Path, join: usize, per_book: usize, limit: Option<usize>) -> Result<()> {
    anyhow::ensure!(join > 0 && per_book > 0, "join and per-book must be positive");
    fs::create_dir_all(out)?;
    let mut written = 0usize;
    for (book, files) in books(chunks)? {
        if files.len() < join {
            continue;
        }
        // Space the samples through the middle half of the book: openings and
        // endings are structurally atypical and would bias temporal features.
        let usable = files.len() - join;
        for i in 0..per_book {
            if limit.is_some_and(|l| written >= l) {
                break;
            }
            let start = usable / 4 + (usable / 2) * i / per_book.max(1);
            let text = files[start..start + join]
                .iter()
                .map(fs::read_to_string)
                .collect::<Result<Vec<_>, _>>()?
                .join("\n\n");
            let name = format!("{}-{start:04}", book.replace(['/', ' '], "_"));
            fs::write(out.join(format!("{name}.txt")), &text)?;
            written += 1;
        }
    }
    println!("wrote {written} passage(s) to {}", out.display());
    Ok(())
}

fn read_passages(dir: &Path) -> Result<Vec<(String, String)>> {
    let mut files: Vec<PathBuf> = fs::read_dir(dir)
        .with_context(|| format!("reading {}", dir.display()))?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.extension().is_some_and(|e| e == "txt"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|p| {
            Ok((
                p.file_stem().unwrap().to_string_lossy().into_owned(),
                fs::read_to_string(p)?,
            ))
        })
        .collect()
}

fn annotate(
    passages: &Path,
    rater: &str,
    endpoints: &[String],
    commands: &[String],
    runs: usize,
    out: &Path,
    abort_after: usize,
) -> Result<()> {
    let backends: Vec<Backend> = endpoints
        .iter()
        .map(|e| Backend::Endpoint(e.clone()))
        .chain(commands.iter().map(|c| Backend::Command(c.clone())))
        .collect();
    anyhow::ensure!(!backends.is_empty(), "give at least one --endpoint or --command");

    let passages = read_passages(passages)?;
    anyhow::ensure!(!passages.is_empty(), "no passages found");
    if let Some(dir) = out.parent() {
        fs::create_dir_all(dir)?;
    }
    let done = load(out)?;
    let jobs = remaining(&passages, runs, rater, &done);
    println!(
        "{rater}: {} passage(s) x {runs} run(s) = {} job(s), {} already done, {} backend(s)",
        passages.len(),
        jobs.len(),
        passages.len() * runs - jobs.len(),
        backends.len()
    );
    if jobs.is_empty() {
        return Ok(());
    }

    let sink = Sink::open(out)?;
    let total = jobs.len();
    let (finished, ok) = (AtomicUsize::new(0), AtomicUsize::new(0));
    let streak = AtomicUsize::new(0);
    let failures: Mutex<Vec<String>> = Mutex::new(Vec::new());

    narrative_features::sweep(jobs, &backends, rater, &sink, |job, result| {
        let n = finished.fetch_add(1, Ordering::Relaxed) + 1;
        match result {
            Ok(a) => {
                ok.fetch_add(1, Ordering::Relaxed);
                streak.store(0, Ordering::Relaxed);
                let bad = malformed(&a.values);
                println!(
                    "[{n}/{total}] {} run {} in {:.0}s{}",
                    job.passage,
                    job.run,
                    a.seconds,
                    match bad.len() {
                        0 => String::new(),
                        k => format!(" — {k} unusable answer(s): {}", bad.join(", ")),
                    }
                );
            }
            Err(e) => {
                let s = streak.fetch_add(1, Ordering::Relaxed) + 1;
                eprintln!("[{n}/{total}] {} run {} failed: {e:#}", job.passage, job.run);
                failures.lock().expect("poisoned").push(format!("{}: {e:#}", job.passage));
                if s >= abort_after {
                    eprintln!(
                        "aborting: {s} consecutive failures — the rater is not answering, \
                         not merely answering badly"
                    );
                    std::process::exit(2);
                }
            }
        }
    })?;

    let ok = ok.load(Ordering::Relaxed);
    println!("\n{ok}/{total} annotation(s) recorded to {}", out.display());
    let failures = failures.into_inner().expect("poisoned");
    if !failures.is_empty() {
        println!("{} failure(s); first few:", failures.len());
        for f in failures.iter().take(5) {
            println!("  {f}");
        }
    }
    Ok(())
}

fn show(annotations: &[Annotation], heading: &str) {
    let mut raters: Vec<&str> = annotations.iter().map(|a| a.rater.as_str()).collect();
    raters.sort_unstable();
    raters.dedup();
    let mut passages: Vec<&str> = annotations.iter().map(|a| a.passage.as_str()).collect();
    passages.sort_unstable();
    passages.dedup();
    println!(
        "\n{heading}\n{} annotation(s), {} passage(s), rater(s): {}\n",
        annotations.len(),
        passages.len(),
        raters.join(", ")
    );

    let reports = report(annotations);
    println!("{:<32} {:>7} {:>7} {:>7} {:>6}  verdict", "feature", "alpha", "exact", "±1", "vals");
    let fmt = |v: Option<f64>| v.map_or("   —  ".into(), |x| format!("{x:6.3}"));
    for r in &reports {
        println!(
            "{:<32} {:>7} {:>6.0}% {:>7} {:>6}  {}",
            r.feature.key,
            fmt(r.alpha),
            r.percent.exact * 100.0,
            r.percent.within_one.map_or("   —  ".into(), |x| format!("{:5.0}%", x * 100.0)),
            r.distinct,
            agree::verdict(r.alpha)
        );
    }

    let scored: Vec<f64> = reports.iter().filter_map(|r| r.alpha).collect();
    if scored.is_empty() {
        println!("\nno feature had enough paired answers to score");
        return;
    }
    let mean = scored.iter().sum::<f64>() / scored.len() as f64;
    let count = |p: fn(f64) -> bool| scored.iter().filter(|a| p(**a)).count();
    println!(
        "\nmean alpha {mean:.3} over {} scored feature(s): {} reliable (>=.80), {} tentative (>=.667), {} below",
        scored.len(),
        count(|a| a >= 0.800),
        count(|a| (0.667..0.800).contains(&a)),
        count(|a| a < 0.667)
    );
    let unscored = reports.len() - scored.len();
    if unscored > 0 {
        println!(
            "{unscored} feature(s) unscored: every rater gave one answer throughout, \
             so the corpus does not exercise them"
        );
    }
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Sample { chunks, out, join, per_book, limit } => {
            sample(&chunks, &out, join, per_book, limit)?
        }
        Command::Annotate { passages, rater, endpoints, commands, runs, out, abort_after } => {
            annotate(&passages, &rater, &endpoints, &commands, runs, &out, abort_after)?
        }
        Command::Report { input, within, raters, first_run_only, aggregate } => {
            let all = load(&input)?;
            anyhow::ensure!(!all.is_empty(), "no annotations in {}", input.display());
            let mut selected: Vec<Annotation> = all
                .iter()
                .filter(|a| raters.is_empty() || raters.contains(&a.rater))
                .filter(|a| !first_run_only || a.run == 1)
                .cloned()
                .collect();
            if aggregate {
                selected = narrative_features::aggregate(&selected);
            }
            match within {
                Some(r) => {
                    let mine: Vec<Annotation> =
                        all.iter().filter(|a| a.rater == r).cloned().collect();
                    anyhow::ensure!(!mine.is_empty(), "no annotations by {r}");
                    show(&mine, &format!("Self-consistency of {r} across its own runs"));
                }
                None => show(&selected, "Concordance between raters"),
            }
            println!(
                "\n{} of StoryScope's 30 core features, questions as published",
                features::FEATURES.len()
            );
        }
    }
    Ok(())
}
