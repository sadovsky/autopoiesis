//! How much of the seeded template-repairing tiling is intact over time, under a given
//! repair_source, noise and sun. Usage: tiling_trace <copy_self|register|previous> <noise> <sun> [seed]
use autopoiesis::config::{RepairSource, SimConfig, SunProfile, TilingPattern};
use autopoiesis::sim::Sim;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let src = match args.get(1).map(|s| s.as_str()) {
        Some("register") => RepairSource::Register,
        Some("previous") => RepairSource::Previous,
        Some("opposite") => RepairSource::Opposite,
        _ => RepairSource::CopySelf,
    };
    let noise: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.001);
    let sun: f64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(3.0);
    let seed: u64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(1);
    let cfg = SimConfig {
        width: 32,
        height: 32,
        sun,
        sun_profile: SunProfile::Uniform,
        noise_rate: noise,
        repair_source: src,
        no_self_jump: true,
        seed_tiling: true,
        seed_tiling_width: 8,
        seed_ring_x: Some(12),
        ..SimConfig::default()
    };
    let reference = {
        let mut g = autopoiesis::grid::Grid::new(32, 32);
        autopoiesis::sim::inject_tiling(&cfg, &mut g);
        g
    };
    let mut sim = Sim::new(cfg.clone(), seed).unwrap();
    let interior: Vec<usize> = (0..32 * 32)
        .filter(|&i| {
            let x = i % 32;
            (13..19).contains(&x) // interior of the band 12..20
        })
        .collect();
    let bg: Vec<usize> = (0..32 * 32).filter(|&i| !(12..20).contains(&(i % 32))).collect();
    let bg_ref = sim.cur.clone();
    for k in 0..=20 {
        let intact = interior.iter().filter(|&&i| sim.cur.cells[i].instr == reference.cells[i].instr).count();
        let bg_same = bg.iter().filter(|&&i| sim.cur.cells[i].instr == bg_ref.cells[i].instr).count();
        println!(
            "t={:6} interior intact {:5.1}%  background unchanged since t0 {:5.1}%  deaths/tick={} repairs/tick={}",
            sim.tick,
            100.0 * intact as f64 / interior.len() as f64,
            100.0 * bg_same as f64 / bg.len() as f64,
            sim.last_step.deaths,
            sim.last_step.repairs
        );
        if k < 20 {
            sim.run(500);
        }
    }
}
