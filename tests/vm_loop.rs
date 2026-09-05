//! Phase 2 acceptance: a hand-written 4-cell repair loop survives 10k ticks at
//! `noise_rate = 0`, heals a corrupted member, and the whole sim is deterministic.

use autopoiesis::config::{SimConfig, SunProfile};
use autopoiesis::grid::{E, Grid};
use autopoiesis::isa::Instruction;
use autopoiesis::sim::Sim;

/// 4x4 torus, all Nop except row 1, which is four `Repair(E)` cells. Each repairs the
/// next one east and the row wraps, so the repair graph is a 4-cycle.
fn ring_world() -> (SimConfig, Grid) {
    let cfg = SimConfig {
        width: 4,
        height: 4,
        sun: 3.0,
        sun_profile: SunProfile::Uniform,
        energy_cap: 1000,
        init_energy_max: 100,
        noise_rate: 0.0,
        ..SimConfig::default()
    };
    let mut g = Grid::new(4, 4);
    for c in &mut g.cells {
        c.instr = Instruction::Nop.encode();
        c.energy = 100;
    }
    for x in 0..4 {
        let c = g.get_mut(x, 1);
        c.instr = Instruction::Repair(E).encode();
        c.tag = 7;
    }
    (cfg, g)
}

fn ring_intact(g: &Grid) -> bool {
    (0..4).all(|x| {
        let c = g.get(x, 1);
        Instruction::decode(c.instr) == Instruction::Repair(E) && c.tag == 7
    })
}

#[test]
fn four_cell_loop_survives_10k_ticks_without_noise() {
    let (cfg, g) = ring_world();
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    for _ in 0..10_000 {
        sim.step();
        assert!(ring_intact(&sim.cur), "ring broke at tick {}", sim.tick);
    }
    assert!(sim.stats.repairs > 0);
    assert_eq!(sim.stats.deaths, 0, "nobody should starve at sun=3");
    // Every ring cell repaired and was repaired: edge set is the 4-cycle.
    let edges = sim.repair_edges();
    let row: Vec<u32> = (0..4).map(|x| sim.cur.idx(x, 1) as u32).collect();
    for (k, &a) in row.iter().enumerate() {
        let b = row[(k + 1) % 4];
        assert!(edges.contains(&(a, b)), "missing edge {a}->{b} in {edges:?}");
    }
}

#[test]
fn corrupted_member_is_healed() {
    let (cfg, g) = ring_world();
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    sim.run(100);
    assert!(ring_intact(&sim.cur));
    // Zap one member with a foreign byte and a foreign tag.
    {
        let c = sim.cur.get_mut(2, 1);
        c.instr = Instruction::Absorb(0).encode();
        c.tag = 99;
    }
    assert!(!ring_intact(&sim.cur));
    // Its western neighbour executes Repair(E) at least once every 9 ticks (when its
    // ip passes over one of the three ring bytes in its neighbourhood), so the ring is
    // whole again within 9 ticks.
    let mut healed_at = None;
    for _ in 0..9 {
        sim.step();
        if ring_intact(&sim.cur) {
            healed_at = Some(sim.tick);
            break;
        }
    }
    assert!(healed_at.is_some(), "ring not healed within 9 ticks");
    sim.run(1000);
    assert!(ring_intact(&sim.cur));
}

#[test]
fn same_seed_same_hash_at_tick_1000() {
    let cfg = SimConfig {
        width: 32,
        height: 32,
        noise_rate: 0.005,
        diffusion: 0.05,
        ..SimConfig::default()
    };
    let mut a = Sim::new(cfg.clone(), 12345).unwrap();
    let mut b = Sim::new(cfg.clone(), 12345).unwrap();
    a.run(1000);
    b.run(1000);
    assert_eq!(a.cur.hash(), b.cur.hash());
    assert_eq!(a.cur, b.cur);
    assert_eq!(a.stats, b.stats);
    let mut c = Sim::new(cfg, 12346).unwrap();
    c.run(1000);
    assert_ne!(a.cur.hash(), c.cur.hash());
}

#[test]
fn write_conflict_highest_energy_wins_then_lowest_index() {
    // Three cells in a row all Store into the same target; the target is south of
    // the middle one and diagonal to the others. Middle has the most energy.
    let cfg = SimConfig {
        width: 5,
        height: 5,
        sun: 0.0,
        noise_rate: 0.0,
        ..SimConfig::default()
    };
    let mut g = Grid::new(5, 5);
    for c in &mut g.cells {
        c.instr = Instruction::Halt.encode();
        c.energy = 50;
    }
    use autopoiesis::grid::{S, SE, SW};
    // Writers at (1,1) SE, (2,1) S, (3,1) SW → target (2,2).
    let mut w = |x: i64, dir: u8, reg: u8, energy: u16| {
        let c = g.get_mut(x, 1);
        c.instr = Instruction::Store(dir).encode();
        c.reg = reg;
        c.energy = energy;
    };
    w(1, SE, 0xA1, 100);
    w(2, S, 0xB2, 300);
    w(3, SW, 0xC3, 100);
    let mut sim = Sim::with_grid(cfg.clone(), 1, g.clone()).unwrap();
    sim.step();
    assert_eq!(sim.cur.get(2, 2).instr, 0xB2, "highest energy wins");

    // Tie between (1,1) and (3,1): lowest index wins.
    let c = g.get_mut(2, 1);
    c.energy = 100;
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    sim.step();
    assert_eq!(sim.cur.get(2, 2).instr, 0xA1, "lowest index wins ties");
    assert_eq!(sim.stats.write_conflicts, 2);
}
