//! `hgt` — run the sandbox, or re-derive its metrics from an event log.

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use hgt::config::{HgtConfig, Mechanisms};
use hgt::event::Event;
use hgt::metrics::{Analyzer, Record};
use hgt::node::Policy;
use hgt::world::World;
use rayon::prelude::*;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[derive(Parser, Debug)]
#[command(name = "hgt", about = "Horizontal gene transfer between networked programs")]
struct Cli {
    #[command(flatten)]
    run: RunArgs,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Args, Debug, Clone)]
struct RunArgs {
    /// Run seed. Fixes the population, the stressor schedule and every draw in the run.
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Ticks to run.
    #[arg(long, default_value_t = 3000)]
    ticks: u32,
    /// Load a JSON config; any subset of the fields, the rest defaulted.
    #[arg(long)]
    config: Option<PathBuf>,

    /// Transfer mechanisms: `none`, `all`, or a list of `conj`, `transf`, `transd`.
    #[arg(long, value_parser = Mechanisms::parse)]
    hgt: Option<Mechanisms>,
    /// What nodes offer and accept.
    #[arg(long, value_enum)]
    policy: Option<Policy>,
    /// Nodes in the founding population.
    #[arg(long)]
    nodes: Option<usize>,
    /// Ceiling on live nodes.
    #[arg(long)]
    max_nodes: Option<usize>,
    /// Peers each node starts with.
    #[arg(long)]
    degree: Option<usize>,
    /// Ticks between stressor shifts.
    #[arg(long)]
    epoch_ticks: Option<u32>,
    /// Distinct stressors.
    #[arg(long)]
    hazard_kinds: Option<u8>,
    /// Strength of the restriction barrier, 0..1.
    #[arg(long)]
    restriction: Option<f64>,
    /// Delivery delay in ticks.
    #[arg(long)]
    latency: Option<u32>,
    /// Message loss, 0..1.
    #[arg(long)]
    loss: Option<f64>,
    /// Per-byte mutation probability when a genome is copied.
    #[arg(long)]
    mutation_rate: Option<f64>,
    /// Genes a node can hold.
    #[arg(long)]
    max_genes: Option<usize>,

    /// Write metric records here, one JSON object per line ("-" for stdout).
    #[arg(long)]
    metrics: Option<PathBuf>,
    /// Write the raw event stream here — the input `analyze` re-derives metrics from.
    #[arg(long)]
    events: Option<PathBuf>,
    /// Ticks between metric frames.
    #[arg(long)]
    analysis_every: Option<u32>,
    /// Ticks between *emitted* frames; must be a multiple of `analysis_every`.
    #[arg(long)]
    report_every: Option<u32>,
    /// Progress to stderr every N ticks.
    #[arg(long, default_value_t = 0)]
    progress: u32,
    /// Print the effective config as JSON and exit.
    #[arg(long)]
    dump_config: bool,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Re-derive metrics from an event log written by `--events`. Needs the same
    /// `--seed` (and `--config`, if one was used): the stressor schedule, and therefore
    /// which genes are resistance genes, is a function of both.
    Analyze {
        file: PathBuf,
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run many seeds in parallel into a directory.
    Sweep {
        /// `a..b` (half-open) or a comma list.
        #[arg(long)]
        seeds: String,
        #[arg(long)]
        out: PathBuf,
        #[arg(long)]
        jobs: Option<usize>,
    },
}

fn build_config(a: &RunArgs) -> Result<HgtConfig> {
    let mut cfg = match &a.config {
        Some(path) => HgtConfig::load(path)?,
        None => HgtConfig::default(),
    };
    if let Some(m) = a.hgt {
        cfg.mechanisms = m;
    }
    if let Some(p) = a.policy {
        cfg.policy = p;
    }
    if let Some(n) = a.nodes {
        cfg.nodes = n;
    }
    if let Some(n) = a.max_nodes {
        cfg.max_nodes = n;
    }
    if let Some(d) = a.degree {
        cfg.degree = d;
    }
    if let Some(e) = a.epoch_ticks {
        cfg.epoch_ticks = e;
    }
    if let Some(k) = a.hazard_kinds {
        cfg.hazard_kinds = k;
    }
    if let Some(r) = a.restriction {
        cfg.restriction = r;
    }
    if let Some(l) = a.latency {
        cfg.latency = l;
    }
    if let Some(l) = a.loss {
        cfg.loss = l;
    }
    if let Some(m) = a.mutation_rate {
        cfg.mutation_rate = m;
    }
    if let Some(g) = a.max_genes {
        cfg.max_genes = g;
    }
    if let Some(n) = a.analysis_every {
        cfg.analysis_every = n;
    }
    if let Some(n) = a.report_every {
        cfg.report_every = n;
    }
    cfg.validate()?;
    Ok(cfg)
}

/// One JSON object per line, to a file or to stdout.
struct Jsonl {
    w: BufWriter<Box<dyn Write + Send>>,
}

impl Jsonl {
    fn file(path: &Path) -> Result<Jsonl> {
        if path == Path::new("-") {
            return Ok(Jsonl::stdout());
        }
        if let Some(parent) = path.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)
                .with_context(|| format!("creating {}", parent.display()))?;
        }
        let f = File::create(path).with_context(|| format!("creating {}", path.display()))?;
        Ok(Jsonl { w: BufWriter::new(Box::new(f)) })
    }

    fn stdout() -> Jsonl {
        Jsonl { w: BufWriter::new(Box::new(std::io::stdout())) }
    }

    fn write<T: Serialize>(&mut self, rec: &T) -> Result<()> {
        serde_json::to_writer(&mut self.w, rec)?;
        self.w.write_all(b"\n")?;
        Ok(())
    }

    fn flush(&mut self) -> Result<()> {
        self.w.flush()?;
        Ok(())
    }
}

/// The one-line answer to "what happened in this run".
#[derive(Serialize, Debug, Clone)]
struct RunSummary {
    kind: &'static str,
    seed: u64,
    ticks: u32,
    mechanisms: String,
    policy: String,
    population: usize,
    extinct_at: Option<u32>,
    epochs_survived: u32,
    births: u64,
    deaths: u64,
    attempts: u64,
    transfers: u64,
    lateral_share: f64,
    /// Ticks from each stressor shift to the answering gene reaching fixation.
    rescue_ticks: Vec<Option<u32>>,
    hash: String,
}

/// Where records go, and the few running totals the summary line needs.
struct Collector {
    metrics: Option<Jsonl>,
    report_every: u32,
    epochs_survived: u32,
    rescue_ticks: Vec<Option<u32>>,
    lateral_share: f64,
}

impl Collector {
    fn take(&mut self, records: Vec<Record>) -> Result<()> {
        for rec in records {
            match &rec {
                Record::Frame(f) => {
                    self.lateral_share = f.lateral_share;
                    // Frames are computed every `analysis_every` ticks and written every
                    // `report_every`: measuring often is cheap, writing often is not.
                    if !f.tick.is_multiple_of(self.report_every) {
                        continue;
                    }
                }
                Record::Epoch(e) => {
                    if e.survived {
                        self.epochs_survived += 1;
                    }
                    self.rescue_ticks.push(e.rescue_ticks);
                }
                Record::Gene(_) => {}
            }
            if let Some(m) = &mut self.metrics {
                m.write(&rec)?;
            }
        }
        Ok(())
    }
}

fn run(cfg: HgtConfig, a: &RunArgs) -> Result<RunSummary> {
    let mut world = World::new(cfg.clone(), a.seed)?;
    let mut analyzer = Analyzer::new(&cfg, a.seed);
    let mut events = a.events.as_deref().map(Jsonl::file).transpose()?;
    let mut out = Collector {
        metrics: a.metrics.as_deref().map(Jsonl::file).transpose()?,
        report_every: cfg.report_every,
        epochs_survived: 0,
        rescue_ticks: Vec::new(),
        lateral_share: 0.0,
    };
    let mut extinct_at = None;

    for e in world.founding_events() {
        if let Some(w) = &mut events {
            w.write(&e)?;
        }
        out.take(analyzer.observe(&e))?;
    }

    for t in 0..a.ticks {
        for e in world.step() {
            if let Some(w) = &mut events {
                w.write(&e)?;
            }
            out.take(analyzer.observe(&e))?;
        }
        if a.progress > 0 && t.is_multiple_of(a.progress) {
            eprintln!("tick {t}: {} nodes", world.population());
        }
        if world.extinct() {
            extinct_at = Some(t);
            break;
        }
    }
    out.take(analyzer.finish())?;

    let summary = RunSummary {
        kind: "summary",
        seed: a.seed,
        ticks: world.tick,
        mechanisms: cfg.mechanisms.label(),
        policy: format!("{:?}", cfg.policy).to_lowercase(),
        population: world.population(),
        extinct_at,
        epochs_survived: out.epochs_survived,
        births: world.stats.births,
        deaths: world.stats.deaths,
        attempts: world.stats.attempts,
        transfers: world.stats.transfers,
        lateral_share: out.lateral_share,
        rescue_ticks: out.rescue_ticks.clone(),
        hash: format!("{:016x}", world.hash()),
    };
    if let Some(m) = &mut out.metrics {
        m.write(&summary)?;
        m.flush()?;
    }
    if let Some(w) = &mut events {
        w.flush()?;
    }
    Ok(summary)
}

fn analyze(cfg: &HgtConfig, seed: u64, file: &Path, out: Option<&Path>) -> Result<()> {
    let f = File::open(file).with_context(|| format!("opening {}", file.display()))?;
    let mut analyzer = Analyzer::new(cfg, seed);
    let mut sink = match out {
        Some(p) => Jsonl::file(p)?,
        None => Jsonl::stdout(),
    };
    for (i, line) in BufReader::new(f).lines().enumerate() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let event: Event = serde_json::from_str(&line)
            .with_context(|| format!("{}:{}: not an event", file.display(), i + 1))?;
        for rec in analyzer.observe(&event) {
            if let Record::Frame(f) = &rec
                && !f.tick.is_multiple_of(cfg.report_every)
            {
                continue;
            }
            sink.write(&rec)?;
        }
    }
    for rec in analyzer.finish() {
        sink.write(&rec)?;
    }
    sink.flush()
}

fn parse_seeds(s: &str) -> Result<Vec<u64>> {
    if let Some((a, b)) = s.split_once("..") {
        let a: u64 = a.trim().parse().with_context(|| format!("seed range start `{a}`"))?;
        let b: u64 = b.trim().parse().with_context(|| format!("seed range end `{b}`"))?;
        if b <= a {
            bail!("seed range `{s}` is empty");
        }
        return Ok((a..b).collect());
    }
    s.split(',')
        .map(|p| p.trim().parse::<u64>().with_context(|| format!("seed `{p}`")))
        .collect()
}

fn sweep(cfg: HgtConfig, a: &RunArgs, seeds: &[u64], out: &Path, jobs: Option<usize>) -> Result<()> {
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    fs::write(out.join("config.json"), serde_json::to_string_pretty(&cfg)? + "\n")?;
    if let Some(j) = jobs {
        rayon::ThreadPoolBuilder::new().num_threads(j).build_global().ok();
    }
    let summaries = Mutex::new(Vec::new());
    let failures = Mutex::new(Vec::new());
    seeds.par_iter().for_each(|&seed| {
        let mut args = a.clone();
        args.seed = seed;
        args.metrics = Some(out.join(format!("seed_{seed}.jsonl")));
        args.events = None;
        args.progress = 0;
        match run(cfg.clone(), &args) {
            Ok(s) => summaries.lock().expect("lock").push(s),
            Err(e) => failures.lock().expect("lock").push(format!("seed {seed}: {e:#}")),
        }
    });
    let mut summaries = summaries.into_inner().unwrap_or_default();
    summaries.sort_by_key(|s| s.seed);
    let mut sink = Jsonl::file(&out.join("summary.jsonl"))?;
    for s in &summaries {
        sink.write(s)?;
    }
    sink.flush()?;
    let survived = summaries.iter().filter(|s| s.extinct_at.is_none()).count();
    eprintln!("{} seeds, {survived} still alive at the end -> {}", summaries.len(), out.display());
    let failures = failures.into_inner().unwrap_or_default();
    if !failures.is_empty() {
        bail!("{} runs failed:\n{}", failures.len(), failures.join("\n"));
    }
    Ok(())
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = build_config(&cli.run)?;
    if cli.run.dump_config {
        println!("{}", serde_json::to_string_pretty(&cfg)?);
        return Ok(());
    }
    match &cli.cmd {
        Some(Cmd::Analyze { file, out }) => analyze(&cfg, cli.run.seed, file, out.as_deref()),
        Some(Cmd::Sweep { seeds, out, jobs }) => {
            let seeds = parse_seeds(seeds)?;
            sweep(cfg, &cli.run, &seeds, out, *jobs)
        }
        None => {
            let s = run(cfg, &cli.run)?;
            println!(
                "seed={} hgt={} ticks={} population={} epochs_survived={} births={} deaths={} \
                 transfers={}/{} lateral_share={:.3} hash={}",
                s.seed,
                s.mechanisms,
                s.ticks,
                s.population,
                s.epochs_survived,
                s.births,
                s.deaths,
                s.transfers,
                s.attempts,
                s.lateral_share,
                s.hash
            );
            Ok(())
        }
    }
}
