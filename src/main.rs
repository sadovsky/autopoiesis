use anyhow::Result;
use autopoiesis::config::{NoiseRamp, SimConfig};
use autopoiesis::sim::Sim;
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
    /// Print the effective config as JSON and exit.
    #[arg(long)]
    dump_config: bool,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
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
    cfg.validate()?;
    if cli.dump_config {
        println!("{}", serde_json::to_string_pretty(&cfg)?);
        return Ok(());
    }

    let mut sim = Sim::new(cfg, cli.seed)?;
    let start = std::time::Instant::now();
    sim.run(cli.ticks);
    let dt = start.elapsed();
    println!(
        "seed={} ticks={} hash={:016x} energy={} repairs={} deaths={} mutations={} elapsed={:.2}s",
        sim.seed,
        sim.tick,
        sim.cur.hash(),
        sim.cur.total_energy(),
        sim.stats.repairs,
        sim.stats.deaths,
        sim.stats.mutations,
        dt.as_secs_f64()
    );
    Ok(())
}
