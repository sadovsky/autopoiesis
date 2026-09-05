//! Trace the seeded ring column over time: glyphs, energies, SCC sizes.
use autopoiesis::config::{SimConfig, SunProfile};
use autopoiesis::isa::Instruction;
use autopoiesis::metrics::find_organisms;
use autopoiesis::sim::Sim;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let width: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(1);
    let noise: f64 = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(0.001);
    let seed: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(4);
    let cx: i64 = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(12);
    let cfg = SimConfig {
        width: 24,
        height: 24,
        sun: 4.0,
        sun_profile: SunProfile::Uniform,
        noise_rate: noise,
        seed_ring: true,
        seed_ring_x: Some(12),
        seed_ring_width: width,
        ..SimConfig::default()
    };
    let mut sim = Sim::new(cfg.clone(), seed).unwrap();
    for _ in 0..=30 {
        let col: String = (0..24)
            .map(|y| {
                let c = sim.cur.get(cx, y);
                if c.energy == 0 { ' ' } else { Instruction::decode(c.instr).glyph() }
            })
            .collect();
        let energies: Vec<u16> = (0..24).map(|y| sim.cur.get(cx, y).energy).collect();
        let ips: String = (0..24).map(|y| char::from_digit(sim.cur.get(cx, y).ip as u32 % 9, 10).unwrap()).collect();
        let edges = sim.repair_edges();
        let orgs = find_organisms(sim.cur.len(), &edges, 3);
        let mut sizes: Vec<usize> = orgs.iter().map(|o| o.core.len()).collect();
        sizes.sort_unstable_by(|a, b| b.cmp(a));
        println!(
            "t={:4} col=[{}] ip=[{}] e(min/med)={}/{} deaths={} sizes={:?}",
            sim.tick,
            col,
            ips,
            energies.iter().min().unwrap(),
            { let mut e = energies.clone(); e.sort(); e[12] },
            sim.last_step.deaths,
            &sizes[..sizes.len().min(6)]
        );
        sim.run(20);
    }
}
