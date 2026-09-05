//! The headline experiment on stdout: one population, one seed, one stressor schedule,
//! run once with transfer off and once with it on.
//!
//! `cargo run --release -p hgt --example ab -- [seed] [ticks]`

use hgt::config::{HgtConfig, Mechanisms};
use hgt::world::World;

fn main() {
    let mut args = std::env::args().skip(1);
    let seed: u64 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1);
    let ticks: u32 = args.next().and_then(|s| s.parse().ok()).unwrap_or(1200);

    for mechanisms in [Mechanisms::none(), Mechanisms::default()] {
        let cfg = HgtConfig { mechanisms, ..HgtConfig::default() };
        let epoch = cfg.epoch_ticks;
        let mut w = World::new(cfg, seed).expect("valid config");
        print!("{:>6}  ", mechanisms.label());
        for t in 0..ticks {
            w.step();
            if (t + 1) % epoch == 0 {
                print!("epoch {} -> {:4} nodes   ", t / epoch, w.population());
            }
            if w.extinct() {
                print!("extinct at {t}   ");
                break;
            }
        }
        println!(
            "| transfers {} of {} attempts",
            w.stats.transfers, w.stats.attempts
        );
    }
}
