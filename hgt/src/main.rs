//! `hgt` — run the sandbox, or re-derive its metrics from an event log.

use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};
use hgt::config::{FounderGenes, HgtConfig, Mechanisms};
use hgt::event::Event;
use hgt::metrics::{Analyzer, Record};
use hgt::node::Policy;
use hgt::render::{Renderer, ShowMode, status};
use hgt::tcp::{ID_STRIDE, TcpTransport, founder_ids, id_base};
use hgt::world::World;
use rayon::prelude::*;
use serde::Serialize;
use std::fs::{self, File};
use std::io::{BufRead, BufReader, BufWriter, Write};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;
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
    /// Credit a near miss earns, 0..1. At 0 a stressor is answered exactly or not at all.
    #[arg(long)]
    hazard_gradient: Option<f64>,
    /// Payloads a stressor is posed with each tick.
    #[arg(long)]
    probes: Option<u32>,
    /// Energy every node receives each tick regardless of what it can answer.
    #[arg(long)]
    income: Option<u32>,
    /// What founders start holding.
    #[arg(long, value_enum)]
    founder_genes: Option<FounderGenes>,
    /// How far a near-miss founder's gene starts from working, in bit flips.
    #[arg(long)]
    founder_miss_bits: Option<u32>,
    /// Founders holding the gene for each future stressor.
    #[arg(long)]
    founder_carriers: Option<usize>,
    /// Founders that start as free riders.
    #[arg(long)]
    selfish_founders: Option<usize>,
    /// Probability a child's policy differs from its parent's.
    #[arg(long)]
    policy_drift: Option<f64>,
    /// Probability surviving a phage teaches a node to refuse its gene.
    #[arg(long)]
    crispr_rate: Option<f64>,
    /// Offer peers genes nobody has seen work, junk included.
    #[arg(long)]
    offer_unproven: bool,
    /// Tick at which the network splits in two.
    #[arg(long)]
    partition_at: Option<u32>,
    /// Tick at which it heals.
    #[arg(long)]
    partition_heal_at: Option<u32>,
    /// Share of the id space on the far side of the split.
    #[arg(long)]
    partition_frac: Option<f64>,

    /// Watch the population in the terminal.
    #[arg(long)]
    render: bool,
    /// What the renderer paints.
    #[arg(long, value_enum, default_value_t = ShowMode::Resistance)]
    show: ShowMode,
    /// Frames per second cap; 0 for uncapped.
    #[arg(long, default_value_t = 30.0)]
    fps: f64,
    /// Draw every N ticks.
    #[arg(long, default_value_t = 1)]
    render_every: u32,

    /// Write metric records here, one JSON object per line ("-" for stdout).
    #[arg(long)]
    metrics: Option<PathBuf>,
    /// Write the family tree and the transfer graph into this directory: ancestry.tsv,
    /// ancestry.newick and transfers.tsv. Every row in transfers.tsv is a place where a
    /// gene's history departs from the tree.
    #[arg(long)]
    trees: Option<PathBuf>,
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
    /// One process holding one deme, talking to the others over TCP. Started by `arena`,
    /// but runnable by hand: several of these on one machine (or several machines) are a
    /// real network of programs trading genes.
    Node {
        /// This process's index. Its nodes are numbered from `index * 2^24`.
        #[arg(long)]
        index: u32,
        /// Address to listen on.
        #[arg(long)]
        listen: SocketAddr,
        /// Every process's address, in index order — including this one.
        #[arg(long, value_delimiter = ',')]
        peers: Vec<SocketAddr>,
        /// Milliseconds per tick. Real messages need real time to arrive.
        #[arg(long, default_value_t = 5)]
        tick_ms: u64,
    },
    /// Spawn `processes` node processes on localhost and watch them trade genes.
    Arena {
        #[arg(long, default_value_t = 4)]
        processes: u32,
        #[arg(long, default_value_t = 9000)]
        base_port: u16,
        #[arg(long, default_value_t = 5)]
        tick_ms: u64,
        /// Where to put the shared config and each process's metrics.
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
    if let Some(g) = a.hazard_gradient {
        cfg.hazard_gradient = g;
    }
    if let Some(p) = a.probes {
        cfg.probes = p;
    }
    if let Some(i) = a.income {
        cfg.income = i;
    }
    if let Some(f) = a.founder_genes {
        cfg.founder_genes = f;
    }
    if let Some(c) = a.founder_carriers {
        cfg.founder_carriers = c;
    }
    if let Some(b) = a.founder_miss_bits {
        cfg.founder_miss_bits = b;
    }
    if let Some(n) = a.selfish_founders {
        cfg.selfish_founders = n;
    }
    if let Some(d) = a.policy_drift {
        cfg.policy_drift = d;
    }
    if let Some(c) = a.crispr_rate {
        cfg.crispr_rate = c;
    }
    if a.offer_unproven {
        cfg.offer_unproven = true;
    }
    if let Some(t) = a.partition_at {
        cfg.partition_at = Some(t);
    }
    if let Some(t) = a.partition_heal_at {
        cfg.partition_heal_at = Some(t);
    }
    if let Some(f) = a.partition_frac {
        cfg.partition_frac = f;
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
    /// The user quit the renderer before the run finished.
    interrupted: bool,
    epochs_survived: u32,
    births: u64,
    deaths: u64,
    attempts: u64,
    transfers: u64,
    lateral_share: f64,
    /// Mutated genes that turned out to answer a stressor, and the first tick one did.
    discoveries: u64,
    novel_discoveries: u64,
    first_discovery: Option<u32>,
    /// Nodes holding a gene that answers each stressor, at the end.
    solvers: Vec<u32>,
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
    discoveries: u64,
    novel_discoveries: u64,
    first_discovery: Option<u32>,
    solvers: Vec<u32>,
}

impl Collector {
    fn take(&mut self, records: Vec<Record>) -> Result<()> {
        for rec in records {
            match &rec {
                Record::Frame(f) => {
                    self.lateral_share = f.lateral_share;
                    self.discoveries = f.discoveries;
                    self.novel_discoveries = f.novel_discoveries;
                    self.solvers = f.solvers.clone();
                    if f.discoveries > 0 && self.first_discovery.is_none() {
                        self.first_discovery = Some(f.tick);
                    }
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
        discoveries: 0,
        novel_discoveries: 0,
        first_discovery: None,
        solvers: Vec::new(),
    };
    let mut extinct_at = None;
    let mut renderer = if a.render { Some(Renderer::new(a.show, a.fps)?) } else { None };
    let render_every = a.render_every.max(1);
    let mut interrupted = false;

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
        if let Some(r) = &mut renderer
            && t.is_multiple_of(render_every)
            && !r.frame(&world, &status(&world, out.lateral_share))?
        {
            interrupted = true;
            break;
        }
        if a.progress > 0 && renderer.is_none() && t.is_multiple_of(a.progress) {
            eprintln!("tick {t}: {} nodes", world.population());
        }
        if world.extinct() {
            extinct_at = Some(t);
            break;
        }
    }
    drop(renderer);
    out.take(analyzer.finish())?;


    let summary = RunSummary {
        kind: "summary",
        seed: a.seed,
        ticks: world.tick,
        mechanisms: cfg.mechanisms.label(),
        policy: format!("{:?}", cfg.policy).to_lowercase(),
        population: world.population(),
        extinct_at,
        interrupted,
        epochs_survived: out.epochs_survived,
        births: world.stats.births,
        deaths: world.stats.deaths,
        attempts: world.stats.attempts,
        transfers: world.stats.transfers,
        lateral_share: out.lateral_share,
        discoveries: out.discoveries,
        novel_discoveries: out.novel_discoveries,
        first_discovery: out.first_discovery,
        solvers: out.solvers.clone(),
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
    if let Some(dir) = &a.trees {
        write_trees(analyzer.trees(), dir)?;
    }
    Ok(summary)
}

/// The family tree and the transfer graph, as files other tools can read.
fn write_trees(trees: &hgt::metrics::Trees, dir: &Path) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("creating {}", dir.display()))?;
    fs::write(dir.join("ancestry.tsv"), trees.ancestry_tsv())?;
    fs::write(dir.join("transfers.tsv"), trees.transfers_tsv())?;
    fs::write(dir.join("ancestry.newick"), trees.newick())?;
    eprintln!(
        "{} nodes, {} lateral transfers -> {}",
        trees.ancestry.len(),
        trees.transfers.len(),
        dir.display()
    );
    Ok(())
}

fn analyze(
    cfg: &HgtConfig,
    seed: u64,
    file: &Path,
    out: Option<&Path>,
    trees: Option<&Path>,
) -> Result<()> {
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
    sink.flush()?;
    if let Some(dir) = trees {
        write_trees(analyzer.trees(), dir)?;
    }
    Ok(())
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
        args.trees = None;
        args.progress = 0;
        args.render = false;
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


/// What a node process reports when it is done. Printed as one JSON line on stdout so
/// the arena — or a shell pipeline — can read it.
#[derive(Serialize, serde::Deserialize, Debug, Clone)]
struct NodeSummary {
    /// A `String`, not a `&'static str` like the other records: the arena reads these
    /// back off its children's stdout, so this one has to deserialize.
    kind: String,
    index: u32,
    seed: u64,
    ticks: u32,
    population: usize,
    births: u64,
    deaths: u64,
    /// Acquisitions whose donor was a node in another process: genes that crossed a
    /// socket. This is the number the whole TCP path exists to produce.
    from_other_processes: u64,
    acquisitions: u64,
    /// Envelopes this process handed to the network, and ones it could not deliver.
    sent: u64,
    dropped: u64,
}

fn node(cfg: HgtConfig, a: &RunArgs, index: u32, listen: SocketAddr, peers: &[SocketAddr], tick_ms: u64) -> Result<()> {
    let transport = TcpTransport::bind(index, listen, peers.to_vec())
        .with_context(|| format!("process {index} listening on {listen}"))?;
    let base = id_base(index);
    let mut world =
        World::with_deme(cfg.clone(), a.seed, Box::new(transport), base..base + ID_STRIDE)?;
    // Every process can work out its peers' founders, so the demes start connected
    // without a coordinator to introduce them.
    let remote: Vec<u32> = (0..peers.len() as u32)
        .filter(|i| *i != index)
        .flat_map(|i| founder_ids(i, cfg.nodes))
        .collect();
    world.introduce(&remote, 2);

    let mut analyzer = Analyzer::new(&cfg, a.seed);
    let mut out = Collector {
        metrics: a.metrics.as_deref().map(Jsonl::file).transpose()?,
        report_every: cfg.report_every,
        epochs_survived: 0,
        rescue_ticks: Vec::new(),
        lateral_share: 0.0,
        discoveries: 0,
        novel_discoveries: 0,
        first_discovery: None,
        solvers: Vec::new(),
    };
    let (mut foreign, mut acquisitions) = (0u64, 0u64);

    for e in world.founding_events() {
        out.take(analyzer.observe(&e))?;
    }
    for _ in 0..a.ticks {
        for e in world.step() {
            if let Event::Acquire { from: Some(donor), .. } = &e {
                acquisitions += 1;
                if donor / ID_STRIDE != index {
                    foreign += 1;
                }
            }
            out.take(analyzer.observe(&e))?;
        }
        if world.extinct() {
            break;
        }
        std::thread::sleep(Duration::from_millis(tick_ms));
    }
    out.take(analyzer.finish())?;
    if let Some(m) = &mut out.metrics {
        m.flush()?;
    }

    let (sent, dropped) = world.transport.counts();
    let summary = NodeSummary {
        kind: "node".to_string(),
        index,
        seed: a.seed,
        ticks: world.tick,
        population: world.population(),
        births: world.stats.births,
        deaths: world.stats.deaths,
        from_other_processes: foreign,
        acquisitions,
        sent,
        dropped,
    };
    println!("{}", serde_json::to_string(&summary)?);
    Ok(())
}

fn arena(cfg: &HgtConfig, a: &RunArgs, processes: u32, base_port: u16, tick_ms: u64, out: Option<&Path>) -> Result<()> {
    let dir = match out {
        Some(p) => p.to_path_buf(),
        None => std::env::temp_dir().join(format!("hgt-arena-{}", std::process::id())),
    };
    fs::create_dir_all(&dir).with_context(|| format!("creating {}", dir.display()))?;
    // Every process shares the seed, because the stressor schedule is derived from it
    // and the demes have to be facing the same world. Only process 0 is founded with the
    // genes for the *later* stressors: everyone else must get them across a socket or
    // die at the shift. That is the experiment.
    let mut cfgs = Vec::new();
    for i in 0..processes {
        let mut c = cfg.clone();
        if i > 0 {
            c.founder_carriers = 0;
        }
        let path = dir.join(format!("config_{i}.json"));
        fs::write(&path, serde_json::to_string_pretty(&c)? + "\n")?;
        cfgs.push(path);
    }

    let addrs: Vec<String> =
        (0..processes).map(|i| format!("127.0.0.1:{}", base_port + i as u16)).collect();
    let peer_list = addrs.join(",");
    let exe = std::env::current_exe().context("finding this executable")?;

    eprintln!(
        "arena: {processes} processes, {} founders each, {} ticks at {tick_ms}ms -> {}",
        cfg.nodes,
        a.ticks,
        dir.display()
    );
    let mut children = Vec::new();
    for i in 0..processes {
        let child = Command::new(&exe)
            .arg("--config").arg(&cfgs[i as usize])
            .arg("--seed").arg(a.seed.to_string())
            .arg("--ticks").arg(a.ticks.to_string())
            .arg("--metrics").arg(dir.join(format!("node_{i}.jsonl")))
            .arg("node")
            .arg("--index").arg(i.to_string())
            .arg("--listen").arg(&addrs[i as usize])
            .arg("--peers").arg(&peer_list)
            .arg("--tick-ms").arg(tick_ms.to_string())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .with_context(|| format!("spawning node {i}"))?;
        children.push(child);
    }

    let mut crossed = 0u64;
    let mut alive = 0usize;
    let mut failures = Vec::new();
    for (i, child) in children.into_iter().enumerate() {
        let output = child.wait_with_output().with_context(|| format!("waiting for node {i}"))?;
        if !output.status.success() {
            failures.push(format!("node {i} exited with {}", output.status));
            continue;
        }
        let text = String::from_utf8_lossy(&output.stdout);
        for line in text.lines() {
            let s: NodeSummary = match serde_json::from_str(line) {
                Ok(s) => s,
                Err(_) => continue,
            };
            crossed += s.from_other_processes;
            alive += s.population;
            println!(
                "process {}: {} nodes, {} acquisitions ({} from another process), \
                 {} envelopes sent, {} undeliverable",
                s.index, s.population, s.acquisitions, s.from_other_processes, s.sent, s.dropped
            );
        }
    }
    println!("{alive} nodes alive; {crossed} genes crossed a socket");
    if !failures.is_empty() {
        bail!("{}", failures.join("\n"));
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
        Some(Cmd::Analyze { file, out }) => {
            analyze(&cfg, cli.run.seed, file, out.as_deref(), cli.run.trees.as_deref())
        }
        Some(Cmd::Node { index, listen, peers, tick_ms }) => {
            node(cfg, &cli.run, *index, *listen, peers, *tick_ms)
        }
        Some(Cmd::Arena { processes, base_port, tick_ms, out }) => {
            arena(&cfg, &cli.run, *processes, *base_port, *tick_ms, out.as_deref())
        }
        Some(Cmd::Sweep { seeds, out, jobs }) => {
            let seeds = parse_seeds(seeds)?;
            sweep(cfg, &cli.run, &seeds, out, *jobs)
        }
        None => {
            let s = run(cfg, &cli.run)?;
            println!(
                "seed={} hgt={} ticks={} population={} epochs_survived={} births={} deaths={} \
                 transfers={}/{} lateral_share={:.3} discovered={} first={} hash={}",
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
                s.discoveries,
                s.first_discovery.map_or("never".to_string(), |t| t.to_string()),
                s.hash
            );
            Ok(())
        }
    }
}
