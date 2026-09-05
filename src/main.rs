use anyhow::{Context, Result};
use autopoiesis::config::{NoiseRamp, SimConfig};
use autopoiesis::render::{Renderer, ShowMode};
use autopoiesis::sim::Sim;
use autopoiesis::snapshot::Snapshot;
use clap::Parser;
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(name = "autopoiesis", about = "A minimal computational-life substrate")]
struct Cli {
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
    /// Inject a hand-written repairing ring at t = 0.
    #[arg(long)]
    seed_ring: bool,
    /// Column for the seeded ring (default: brightest column).
    #[arg(long)]
    seed_ring_x: Option<usize>,
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
    /// Print a progress line every N ticks (0 = never).
    #[arg(long, default_value_t = 0)]
    progress: u32,
}

fn build_config(cli: &Cli) -> Result<SimConfig> {
    let mut cfg = match &cli.config {
        Some(p) => SimConfig::load(p)?,
        None => SimConfig::default(),
    };
    if let Some(w) = cli.width {
        cfg.width = w;
    }
    if let Some(h) = cli.height {
        cfg.height = h;
    }
    if let Some(n) = cli.noise {
        cfg.noise_rate = n;
    }
    if let Some(r) = cli.noise_ramp {
        cfg.noise_ramp = Some(r);
    }
    if cli.seed_ring {
        cfg.seed_ring = true;
    }
    if let Some(x) = cli.seed_ring_x {
        cfg.seed_ring_x = Some(x);
    }
    if let Some(s) = cli.snapshot_every {
        cfg.snapshot_every = s;
    }
    cfg.validate()?;
    Ok(cfg)
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let cfg = build_config(&cli)?;
    if cli.dump_config {
        println!("{}", serde_json::to_string_pretty(&cfg)?);
        return Ok(());
    }

    let mut sim = Sim::new(cfg, cli.seed)?;
    let mut renderer = if cli.render {
        Some(Renderer::new(cli.show, cli.fps)?)
    } else {
        None
    };
    let snapshot_every = sim.cfg.snapshot_every;
    let render_every = cli.render_every.max(1);

    let start = std::time::Instant::now();
    let mut quit = false;
    loop {
        let t = sim.tick;
        if let Some(dir) = &cli.snapshots
            && t % snapshot_every == 0
        {
            write_snapshot(&sim, dir)?;
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
        if cli.progress > 0 && t > 0 && t % cli.progress == 0 && renderer.is_none() {
            eprintln!(
                "t={t} energy={} repairs/tick={} deaths/tick={} elapsed={:.1}s",
                sim.cur.total_energy(),
                sim.last_step.repairs,
                sim.last_step.deaths,
                start.elapsed().as_secs_f64()
            );
        }
        if t >= cli.ticks {
            break;
        }
        sim.step();
    }
    drop(renderer);
    let dt = start.elapsed();
    println!(
        "seed={} ticks={} hash={:016x} energy={} repairs={} deaths={} mutations={} elapsed={:.2}s{}",
        sim.seed,
        sim.tick,
        sim.cur.hash(),
        sim.cur.total_energy(),
        sim.stats.repairs,
        sim.stats.deaths,
        sim.stats.mutations,
        dt.as_secs_f64(),
        if quit { " (interrupted)" } else { "" }
    );
    Ok(())
}

fn write_snapshot(sim: &Sim, dir: &std::path::Path) -> Result<()> {
    let snap = Snapshot {
        tick: sim.tick,
        noise_rate: sim.noise_rate(),
        grid: sim.cur.clone(),
        edges: sim.repair_log.edges(),
    };
    let path = Snapshot::path_for(dir, sim.tick);
    snap.write(&path).with_context(|| format!("writing snapshot {}", path.display()))
}
