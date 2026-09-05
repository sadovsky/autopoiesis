use autopoiesis::config::{SimConfig, SunProfile};
use autopoiesis::grid::Grid;
use autopoiesis::isa::Instruction;
use autopoiesis::sim::Sim;

fn small_cfg() -> SimConfig {
    SimConfig {
        width: 24,
        height: 24,
        noise_rate: 0.01,
        ..SimConfig::default()
    }
}

#[test]
fn with_no_sun_total_energy_never_increases() {
    let cfg = SimConfig {
        sun: 0.0,
        ..small_cfg()
    };
    let mut sim = Sim::new(cfg, 3).unwrap();
    let mut prev = sim.cur.total_energy();
    assert!(prev > 0);
    for _ in 0..2000 {
        sim.step();
        let now = sim.cur.total_energy();
        assert!(now <= prev, "tick {}: energy rose {} -> {}", sim.tick, prev, now);
        prev = now;
    }
    // Everything spends itself eventually; the grid runs dry.
    assert!(prev < 50, "expected the grid to run dry, still {prev}");
}

#[test]
fn with_no_sun_and_diffusion_total_energy_never_increases() {
    let cfg = SimConfig {
        sun: 0.0,
        diffusion: 0.25,
        ..small_cfg()
    };
    let mut sim = Sim::new(cfg, 5).unwrap();
    let mut prev = sim.cur.total_energy();
    for _ in 0..500 {
        sim.step();
        let now = sim.cur.total_energy();
        assert!(now <= prev, "tick {}: energy rose {} -> {}", sim.tick, prev, now);
        prev = now;
    }
}

#[test]
fn with_sun_and_no_execution_energy_rises_to_cap_and_plateaus() {
    // "No execution": every cell holds Halt (cost 0, no effect) with energy above the
    // halt threshold, so the cells tick but do nothing and spend nothing.
    let cfg = SimConfig {
        width: 16,
        height: 16,
        sun: 2.5,
        sun_profile: SunProfile::Uniform,
        energy_cap: 500,
        halt_threshold: 0,
        noise_rate: 0.0,
        ..SimConfig::default()
    };
    let mut grid = Grid::new(16, 16);
    for c in &mut grid.cells {
        c.instr = Instruction::Halt.encode();
        c.energy = 10;
    }
    let mut sim = Sim::with_grid(cfg.clone(), 1, grid).unwrap();
    let cap_total = cfg.n_cells() as u64 * cfg.energy_cap as u64;
    let mut prev = sim.cur.total_energy();
    let mut reached_cap_at = None;
    for _ in 0..1000 {
        sim.step();
        let now = sim.cur.total_energy();
        assert!(now >= prev, "tick {}: energy fell {} -> {}", sim.tick, prev, now);
        assert!(now <= cap_total);
        if now == cap_total && reached_cap_at.is_none() {
            reached_cap_at = Some(sim.tick);
        }
        prev = now;
    }
    let t = reached_cap_at.expect("never reached cap");
    // (500 - 10) / 2.5 = 196 ticks.
    assert!((190..=200).contains(&t), "reached cap at tick {t}");
    assert_eq!(sim.cur.total_energy(), cap_total);
    assert_eq!(sim.stats.deaths, 0);
}
