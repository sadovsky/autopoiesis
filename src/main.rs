use anyhow::{Context, Result, bail};
use autopoiesis::config::{NoiseRamp, SimConfig, SunProfile};
use autopoiesis::metrics::Analyzer;
use autopoiesis::render::{Renderer, ShowMode};
use autopoiesis::sim::Sim;
use autopoiesis::snapshot::{Snapshot, list_snapshots};
use clap::{Args, Parser, Subcommand};
use rayon::prelude::*;
use serde::Serialize;
use std::fs;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Instant;

#[derive(Parser, Debug)]
#[command(name = "autopoiesis", about = "A minimal computational-life substrate")]
struct Cli {
    #[command(flatten)]
    run: RunArgs,
    #[command(subcommand)]
    cmd: Option<Cmd>,
}

#[derive(Args, Debug, Clone)]
struct RunArgs {
    /// RNG seed; a run is fully determined by (config, seed).
    #[arg(long, default_value_t = 1)]
    seed: u64,
    /// Number of ticks to simulate.
    #[arg(long, default_value_t = 10_000)]
    ticks: u32,
    /// JSON config file (any subset of SimConfig fields).
    #[arg(long)]
    config: Option<PathBuf>,
    /// Override grid width.
    #[arg(long)]
    width: Option<usize>,
    /// Override grid height.
    #[arg(long)]
    height: Option<usize>,
    /// Override constant noise rate.
    #[arg(long)]
    noise: Option<f64>,
    /// Ramp noise linearly: `from:to:ticks`, e.g. `0.0:0.05:1000`.
    #[arg(long, value_parser = NoiseRamp::parse)]
    noise_ramp: Option<NoiseRamp>,
    /// Override the sun intensity at the brightest point.
    #[arg(long)]
    sun: Option<f64>,
    /// Override the sun profile.
    #[arg(long, value_enum)]
    sun_profile: Option<SunProfileArg>,
    /// Override the Repair cost.
    #[arg(long)]
    repair_cost: Option<u16>,
    /// Inject a hand-written repairing ring at t = 0.
    #[arg(long)]
    seed_ring: bool,
    /// Column for the seeded ring (default: brightest column).
    #[arg(long)]
    seed_ring_x: Option<usize>,
    /// Width of the seeded ring band.
    #[arg(long)]
    seed_ring_width: Option<usize>,
    /// Print the effective config as JSON and exit.
    #[arg(long)]
    dump_config: bool,

    /// Render the grid in the terminal while running.
    #[arg(long)]
    render: bool,
    /// What the renderer shows.
    #[arg(long, value_enum, default_value_t = ShowMode::Tag)]
    show: ShowMode,
    /// Redraw every N ticks.
    #[arg(long, default_value_t = 1)]
    render_every: u32,
    /// Frame-rate cap for rendering (0 = unlimited).
    #[arg(long, default_value_t = 30.0)]
    fps: f64,

    /// Write `tick_{n}.bin` snapshots into this directory every `snapshot_every` ticks.
    #[arg(long)]
    snapshots: Option<PathBuf>,
    /// Override `snapshot_every`.
    #[arg(long)]
    snapshot_every: Option<u32>,
    /// Compute metrics online every `analysis_every` ticks and append JSON lines here.
    #[arg(long)]
    metrics: Option<PathBuf>,
    /// Override `analysis_every`.
    #[arg(long)]
    analysis_every: Option<u32>,
    /// Print a progress line every N ticks (0 = never).
    #[arg(long, default_value_t = 0)]
    progress: u32,
}

#[derive(Clone, Copy, Debug, clap::ValueEnum)]
enum SunProfileArg {
    Linear,
    Gaussian,
    Uniform,
}

#[derive(Subcommand, Debug)]
enum Cmd {
    /// Analyze a directory of snapshots offline and write JSON-lines metrics. Uses the
    /// top-level `--seed` for the analyzer's own RNG (MI baselines/shuffles) so that
    /// offline results reproduce an online `--metrics` run of the same seed.
    Analyze {
        /// Directory containing `tick_*.bin` files.
        dir: PathBuf,
        /// Output JSONL file (default: stdout).
        #[arg(long)]
        out: Option<PathBuf>,
    },
    /// Run many seeds in parallel; one metrics JSONL per seed plus a summary.
    Sweep {
        /// Seed range `a..b` (half-open) or comma-separated list.
        #[arg(long, default_value = "0..10")]
        seeds: String,
        /// Output directory.
        #[arg(long)]
        out: PathBuf,
        /// Worker threads (default: all cores).
        #[arg(long)]
        jobs: Option<usize>,
    },
}

fn build_config(a: &RunArgs) -> Result<SimConfig> {
    let mut cfg = match &a.config {
        Some(p) => SimConfig::load(p)?,
        None => SimConfig::default(),
    };
    if let Some(w) = a.width {
        cfg.width = w;
    }
    if let Some(h) = a.height {
        cfg.height = h;
    }
    if let Some(n) = a.noise {
        cfg.noise_rate = n;
    }
    if let Some(r) = a.noise_ramp {
        cfg.noise_ramp = Some(r);
    }
    if let Some(s) = a.sun {
        cfg.sun = s;
    }
    if let Some(p) = a.sun_profile {
        cfg.sun_profile = match p {
            SunProfileArg::Linear => SunProfile::Linear,
            SunProfileArg::Gaussian => SunProfile::Gaussian,
            SunProfileArg::Uniform => SunProfile::Uniform,
        };
    }
    if let Some(c) = a.repair_cost {
        cfg.costs.repair = c;
    }
    if a.seed_ring {
        cfg.seed_ring = true;
    }
    if let Some(x) = a.seed_ring_x {
        cfg.seed_ring_x = Some(x);
    }
    if let Some(w) = a.seed_ring_width {
        cfg.seed_ring_width = w;
    }
    if let Some(s) = a.snapshot_every {
        cfg.snapshot_every = s;
    }
    if let Some(s) = a.analysis_every {
        cfg.analysis_every = s;
    }
    cfg.validate()?;
    Ok(cfg)
}

/// Append-only JSON-lines sink.
struct Jsonl {
    w: BufWriter<Box<dyn Write + Send>>,
}

impl Jsonl {
    fn file(path: &Path) -> Result<Jsonl> {
        if let Some(dir) = path.parent()
            && !dir.as_os_str().is_empty()
        {
            fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
        }
        let f = fs::File::create(path).with_context(|| format!("creating {}", path.display()))?;
        Ok(Jsonl {
            w: BufWriter::new(Box::new(f)),
        })
    }
    fn stdout() -> Jsonl {
        Jsonl {
            w: BufWriter::new(Box::new(std::io::stdout())),
        }
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

#[derive(Serialize)]
struct RunSummary {
    kind: &'static str,
    seed: u64,
    ticks: u32,
    hash: String,
    final_energy: u64,
    executed: u64,
    repairs: u64,
    deaths: u64,
    starved: u64,
    mutations: u64,
    write_conflicts: u64,
    organisms_seen: u64,
    elapsed_s: f64,
    interrupted: bool,
}

/// One full run with optional rendering, snapshots and online metrics.
fn run_one(cfg: &SimConfig, a: &RunArgs, metrics: Option<&mut Jsonl>, quiet: bool) -> Result<RunSummary> {
    let mut sim = Sim::new(cfg.clone(), a.seed)?;
    let mut renderer = if a.render {
        Some(Renderer::new(a.show, a.fps)?)
    } else {
        None
    };
    let mut analyzer = metrics.as_ref().map(|_| Analyzer::new(cfg, a.seed));
    let mut metrics = metrics;
    let snapshot_every = cfg.snapshot_every;
    let analysis_every = cfg.analysis_every;
    let render_every = a.render_every.max(1);

    let start = Instant::now();
    let mut quit = false;
    let mut organisms_seen = 0u64;
    loop {
        let t = sim.tick;
        let want_snapshot = a.snapshots.is_some() && t % snapshot_every == 0;
        let want_analysis = analyzer.is_some() && t % analysis_every == 0;
        if want_snapshot || want_analysis {
            let edges = sim.repair_edges();
            if want_snapshot && let Some(dir) = &a.snapshots {
                let snap = Snapshot {
                    tick: t,
                    noise_rate: sim.noise_rate(),
                    grid: sim.cur.clone(),
                    edges: edges.clone(),
                };
                let path = Snapshot::path_for(dir, t);
                snap.write(&path).with_context(|| format!("writing snapshot {}", path.display()))?;
            }
            if want_analysis
                && let (Some(an), Some(sink)) = (&mut analyzer, metrics.as_deref_mut())
            {
                let rep = an.observe(t, sim.noise_rate(), &sim.cur, &edges);
                sink.write(&rep.frame)?;
                for d in &rep.deaths {
                    sink.write(d)?;
                }
                organisms_seen = organisms_seen.max(an.tracked_count() as u64 + rep.deaths.len() as u64);
            }
        }
        if let Some(r) = &mut renderer
            && t % render_every == 0
        {
            let status = format!(
                " t={t} noise={:.4} energy={} repairs/tick={} deaths/tick={}  [q quit, space pause]",
                sim.noise_rate(),
                sim.cur.total_energy(),
                sim.last_step.repairs,
                sim.last_step.deaths
            );
            if !r.frame(&sim, &status)? {
                quit = true;
                break;
            }
        }
        if a.progress > 0 && t > 0 && t % a.progress == 0 && renderer.is_none() && !quiet {
            eprintln!(
                "seed={} t={t} energy={} repairs/tick={} deaths/tick={} elapsed={:.1}s",
                a.seed,
                sim.cur.total_energy(),
                sim.last_step.repairs,
                sim.last_step.deaths,
                start.elapsed().as_secs_f64()
            );
        }
        if t >= a.ticks {
            break;
        }
        sim.step();
    }
    drop(renderer);
    let mut total_organisms = 0u64;
    if let (Some(an), Some(sink)) = (&mut analyzer, metrics.as_deref_mut()) {
        let lives = an.finish();
        for l in &lives {
            sink.write(l)?;
        }
        total_organisms = lives.iter().map(|l| l.organism_id + 1).max().unwrap_or(0);
        sink.flush()?;
    }
    let summary = RunSummary {
        kind: "summary",
        seed: a.seed,
        ticks: sim.tick,
        hash: format!("{:016x}", sim.cur.hash()),
        final_energy: sim.cur.total_energy(),
        executed: sim.stats.executed,
        repairs: sim.stats.repairs,
        deaths: sim.stats.deaths,
        starved: sim.stats.starved,
        mutations: sim.stats.mutations,
        write_conflicts: sim.stats.write_conflicts,
        organisms_seen: total_organisms.max(organisms_seen),
        elapsed_s: start.elapsed().as_secs_f64(),
        interrupted: quit,
    };
    if let Some(sink) = metrics {
        sink.write(&summary)?;
        sink.flush()?;
    }
    Ok(summary)
}

fn parse_seeds(s: &str) -> Result<Vec<u64>> {
    if let Some((a, b)) = s.split_once("..") {
        let a: u64 = a.trim().parse().context("seed range start")?;
        let b: u64 = b.trim().parse().context("seed range end")?;
        if b <= a {
            bail!("empty seed range {s}");
        }
        return Ok((a..b).collect());
    }
    s.split(',')
        .map(|x| x.trim().parse::<u64>().with_context(|| format!("seed `{x}`")))
        .collect()
}

fn analyze(dir: &Path, out: Option<&Path>, seed: u64) -> Result<()> {
    let files = list_snapshots(dir)?;
    if files.is_empty() {
        bail!("no tick_*.bin snapshots in {}", dir.display());
    }
    let first = Snapshot::read(&files[0].1)?;
    let cfg = SimConfig {
        width: first.grid.width,
        height: first.grid.height,
        ..SimConfig::default()
    };
    let mut sink = match out {
        Some(p) => Jsonl::file(p)?,
        None => Jsonl::stdout(),
    };
    let mut an = Analyzer::new(&cfg, seed);
    for (tick, path) in &files {
        let snap = if *tick == first.tick { first.clone() } else { Snapshot::read(path)? };
        let rep = an.observe(snap.tick, snap.noise_rate, &snap.grid, &snap.edges);
        sink.write(&rep.frame)?;
        for d in &rep.deaths {
            sink.write(d)?;
        }
    }
    for l in an.finish() {
        sink.write(&l)?;
    }
    sink.flush()?;
    eprintln!("analyzed {} snapshots from {}", files.len(), dir.display());
    Ok(())
}

fn sweep(cfg: &SimConfig, a: &RunArgs, seeds: &[u64], out: &Path, jobs: Option<usize>) -> Result<()> {
    fs::create_dir_all(out).with_context(|| format!("creating {}", out.display()))?;
    fs::write(out.join("config.json"), serde_json::to_string_pretty(cfg)?)?;
    if let Some(j) = jobs {
        rayon::ThreadPoolBuilder::new().num_threads(j).build_global().ok();
    }
    let start = Instant::now();
    let summaries: Mutex<Vec<RunSummary>> = Mutex::new(Vec::new());
    let errors: Mutex<Vec<String>> = Mutex::new(Vec::new());
    seeds.par_iter().for_each(|&seed| {
        let mut args = a.clone();
        args.seed = seed;
        args.render = false;
        args.snapshots = None;
        let path = out.join(format!("seed_{seed}.jsonl"));
        let result = Jsonl::file(&path).and_then(|mut sink| run_one(cfg, &args, Some(&mut sink), true));
        match result {
            Ok(s) => {
                eprintln!(
                    "seed={seed} done in {:.1}s organisms_seen={} repairs={} deaths={}",
                    s.elapsed_s, s.organisms_seen, s.repairs, s.deaths
                );
                summaries.lock().map(|mut v| v.push(s)).ok();
            }
            Err(e) => {
                errors.lock().map(|mut v| v.push(format!("seed {seed}: {e:#}"))).ok();
            }
        }
    });
    let mut summaries = summaries.into_inner().unwrap_or_default();
    summaries.sort_by_key(|s| s.seed);
    let mut sink = Jsonl::file(&out.join("summary.jsonl"))?;
    for s in &summaries {
        sink.write(s)?;
    }
    sink.flush()?;
    let errors = errors.into_inner().unwrap_or_default();
    eprintln!(
        "sweep: {} seeds in {:.1}s -> {}",
        summaries.len(),
        start.elapsed().as_secs_f64(),
        out.display()
    );
    if !errors.is_empty() {
        bail!("{} runs failed:\n{}", errors.len(), errors.join("\n"));
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
    match cli.cmd {
        Some(Cmd::Analyze { dir, out }) => analyze(&dir, out.as_deref(), cli.run.seed),
        Some(Cmd::Sweep { seeds, out, jobs }) => {
            let seeds = parse_seeds(&seeds)?;
            sweep(&cfg, &cli.run, &seeds, &out, jobs)
        }
        None => {
            let mut sink = match &cli.run.metrics {
                Some(p) => Some(Jsonl::file(p)?),
                None => None,
            };
            let s = run_one(&cfg, &cli.run, sink.as_mut(), false)?;
            println!(
                "seed={} ticks={} hash={} energy={} repairs={} deaths={} mutations={} organisms_seen={} elapsed={:.2}s{}",
                s.seed,
                s.ticks,
                s.hash,
                s.final_energy,
                s.repairs,
                s.deaths,
                s.mutations,
                s.organisms_seen,
                s.elapsed_s,
                if s.interrupted { " (interrupted)" } else { "" }
            );
            Ok(())
        }
    }
}
