//! Phase B: the token execution model.

use autopoiesis::config::{ExecModel, RepairSource, SimConfig, SunProfile, TilingPattern};
use autopoiesis::grid::{E, Grid, S, TOKEN, W};
use autopoiesis::isa::Instruction;
use autopoiesis::sim::Sim;

fn token_cfg() -> SimConfig {
    SimConfig {
        width: 8,
        height: 8,
        sun: 0.0,
        noise_rate: 0.0,
        exec_model: ExecModel::Token,
        token_rate: 0.0,
        token_init: 0.0,
        ..SimConfig::default()
    }
}

#[test]
fn a_token_bounces_between_two_cells_and_only_they_execute() {
    let cfg = token_cfg();
    let mut g = Grid::new(8, 8);
    for c in &mut g.cells {
        c.instr = Instruction::Nop.encode();
        c.energy = 500;
    }
    // A(2,2) -> E -> B(3,2) -> W -> A, one token on A.
    g.get_mut(2, 2).ip = E | TOKEN;
    g.get_mut(3, 2).ip = W;
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    for t in 1..=9u32 {
        sim.step();
        let on_a = sim.cur.get(2, 2).ip & TOKEN != 0;
        let on_b = sim.cur.get(3, 2).ip & TOKEN != 0;
        assert_eq!((on_a, on_b), (t % 2 == 0, t % 2 == 1), "tick {t}");
        assert_eq!(sim.last_step.tokens, 1);
        assert_eq!(sim.last_step.executed, 1);
    }
    assert_eq!(sim.stats.executed, 9);
    // Nobody else spent anything.
    let spent: u64 = sim.cur.cells.iter().map(|c| 500 - c.energy as u64).sum();
    assert_eq!(spent, 9);
}

#[test]
fn reg_travels_with_the_token_and_repair_writes_it() {
    let cfg = SimConfig {
        repair_source: RepairSource::Register,
        ..token_cfg()
    };
    let mut g = Grid::new(8, 8);
    for c in &mut g.cells {
        c.instr = Instruction::Halt.encode();
        c.energy = 500;
    }
    // A(2,2) = Load(W) reads (1,2)=0x77; token -> B(3,2) = Repair(E) writes reg into (4,2).
    g.get_mut(1, 2).instr = 0x77;
    let a = g.get_mut(2, 2);
    a.instr = Instruction::Load(W).encode();
    a.ip = E | TOKEN;
    let b = g.get_mut(3, 2);
    b.instr = Instruction::Repair(E).encode();
    b.ip = W;
    g.get_mut(4, 2).instr = 0x11;
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    sim.step(); // A loads
    assert_eq!(sim.cur.get(3, 2).reg, 0x77, "acc moved with the token");
    sim.step(); // B repairs
    assert_eq!(sim.cur.get(4, 2).instr, 0x77);
    assert_eq!(sim.stats.repairs, 1);
}

#[test]
fn tokens_collide_and_die_with_their_cell() {
    let cfg = token_cfg();
    let mut g = Grid::new(8, 8);
    for c in &mut g.cells {
        c.instr = Instruction::Nop.encode();
        c.energy = 500;
    }
    // Two tokens heading into the same cell (3,2): from (2,2) going E and (4,2) going W.
    g.get_mut(2, 2).ip = E | TOKEN;
    g.get_mut(4, 2).ip = W | TOKEN;
    g.get_mut(3, 2).ip = S;
    let mut sim = Sim::with_grid(cfg.clone(), 1, g).unwrap();
    sim.step();
    assert_eq!(sim.last_step.tokens, 1, "one survives the collision");
    assert_eq!(sim.last_step.tokens_lost, 1);
    assert!(sim.cur.get(3, 2).ip & TOKEN != 0);

    // A token moving onto a dead cell is destroyed; a starving cell loses its token.
    let mut g = Grid::new(8, 8);
    for c in &mut g.cells {
        c.instr = Instruction::Nop.encode();
        c.energy = 500;
    }
    g.get_mut(2, 2).ip = E | TOKEN;
    g.get_mut(3, 2).energy = 0; // dead
    g.get_mut(5, 5).ip = E | TOKEN;
    g.get_mut(5, 5).energy = 0; // dead cells never run
    g.get_mut(6, 6).instr = Instruction::Repair(E).encode(); // costs 4
    g.get_mut(6, 6).energy = 2;
    g.get_mut(6, 6).ip = E | TOKEN;
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    sim.step();
    assert_eq!(sim.last_step.tokens, 0);
    assert_eq!(sim.stats.starved, 1);
    assert_eq!(sim.stats.deaths, 1);
}

#[test]
fn token_model_is_deterministic_and_conserves_energy_without_sun() {
    let cfg = SimConfig {
        width: 32,
        height: 32,
        sun: 0.0,
        noise_rate: 0.002,
        exec_model: ExecModel::Token,
        token_rate: 0.02,
        token_init: 0.1,
        repair_source: RepairSource::Opposite,
        ..SimConfig::default()
    };
    let mut a = Sim::new(cfg.clone(), 5).unwrap();
    let mut b = Sim::new(cfg, 5).unwrap();
    let mut prev = a.cur.total_energy();
    for _ in 0..500 {
        a.step();
        b.step();
        let now = a.cur.total_energy();
        assert!(now <= prev, "energy rose {prev} -> {now}");
        prev = now;
    }
    assert_eq!(a.cur.hash(), b.cur.hash());
    assert!(a.stats.executed > 0 && a.stats.tokens_lost > 0);
}

#[test]
fn pass_through_strips_are_a_fixed_point_but_relay_belts_translate_defects() {
    let cfg = SimConfig {
        width: 16,
        height: 16,
        sun: 1.0,
        sun_profile: SunProfile::Uniform,
        noise_rate: 0.0,
        exec_model: ExecModel::Token,
        token_rate: 0.0,
        token_init: 0.0,
        repair_source: RepairSource::Opposite,
        seed_tiling: true,
        seed_tiling_width: 16,
        seed_tiling_pattern: TilingPattern::PassThrough,
        ..SimConfig::default()
    };
    let reference = Sim::new(cfg.clone(), 3).unwrap().cur.clone();
    let mut sim = Sim::new(cfg.clone(), 3).unwrap();
    assert_eq!(sim.cur.cells.iter().filter(|c| c.ip & TOKEN != 0).count(), 8, "one token per 2-column strip");
    sim.run(3000);
    assert!(sim.cur.cells.iter().zip(&reference.cells).all(|(a, b)| a.instr == b.instr));
    assert_eq!(sim.last_step.tokens, 8);
    assert!(sim.stats.deaths == 0 && sim.stats.repairs > 0);
    let lap = 2 * 16;
    // Relay chains are belts, not repair: every cell of a class is a copy of the cell
    // upstream of it and nothing else, so a defect is relayed, never corrected. Here
    // the token runs against the relay direction, so a single Nop defect floods all
    // eight odd-row cells of its column within one lap and stays; a writing defect
    // (a Store) also destroys relays and spreads across strips.
    let zap = sim.cur.idx(5, 7);
    sim.cur.cells[zap].instr = Instruction::Nop.encode();
    for k in 1..=4 {
        sim.run(lap);
        let broken: Vec<usize> = (0..256).filter(|&i| sim.cur.cells[i].instr != reference.cells[i].instr).collect();
        assert_eq!(broken.len(), 8, "lap {k}: the whole odd class of column 5 should carry the defect, got {broken:?}");
        assert!(broken.iter().all(|&i| i % 16 == 5 && (i / 16) % 2 == 1));
    }
    let mut sim = Sim::new(cfg.clone(), 3).unwrap();
    sim.run(3000);
    sim.cur.cells[zap].instr = Instruction::Store(S).encode();
    sim.cur.cells[zap].reg = 0xEE;
    sim.run(4 * lap);
    let broken = sim.cur.cells.iter().zip(&reference.cells).filter(|(a, b)| a.instr != b.instr).count();
    assert!(broken > 8, "a writing defect on a belt spreads (got {broken} broken)");
    // A junk jump throws the token off the strip; with spontaneous tokens the strip is
    // repopulated and healed.
    let cfg2 = SimConfig { token_rate: 0.05, ..cfg };
    let mut sim = Sim::new(cfg2, 3).unwrap();
    sim.run(100);
    let zap = sim.cur.idx(4, 6);
    sim.cur.cells[zap].instr = Instruction::MoveIp(E).encode();
    sim.run(20 * lap);
    assert!(sim.last_step.tokens >= 8, "spontaneous tokens repopulate the strips");
}
