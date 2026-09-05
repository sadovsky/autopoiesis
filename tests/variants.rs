//! Semantics of the §8 variants: template repair (register / previous) and the
//! no-self-jump rule that breaks the MoveIp trap.

use autopoiesis::config::{RepairSource, SimConfig, SunProfile};
use autopoiesis::grid::{E, Grid, N, NE, W};
use autopoiesis::isa::Instruction;
use autopoiesis::sim::Sim;

fn quiet_world(repair_source: RepairSource, no_self_jump: bool) -> (SimConfig, Grid) {
    let cfg = SimConfig {
        width: 6,
        height: 6,
        sun: 0.0,
        sun_profile: SunProfile::Uniform,
        noise_rate: 0.0,
        repair_source,
        no_self_jump,
        ..SimConfig::default()
    };
    // Every cell starts dead (0 energy, sun 0): it keeps its byte but never executes,
    // so only the cells a test energises act. Bytes are Halt so that a fetched
    // background byte is a free no-op.
    let mut g = Grid::new(6, 6);
    for c in &mut g.cells {
        c.instr = Instruction::Halt.encode();
        c.energy = 0;
    }
    (cfg, g)
}

#[test]
fn repair_source_decides_what_is_written() {
    for (src, expected) in [
        (RepairSource::CopySelf, Instruction::Repair(E).encode()),
        (RepairSource::Register, 0xAB),
        // ip = 0 executes self; the slot before self in neighbourhood order is NW (ip 8).
        (RepairSource::Previous, 0xCD),
    ] {
        let (cfg, mut g) = quiet_world(src, false);
        {
            let c = g.get_mut(2, 2);
            c.instr = Instruction::Repair(E).encode();
            c.reg = 0xAB;
            c.tag = 9;
            c.energy = 500;
        }
        g.get_mut(1, 1).instr = 0xCD; // NW of (2,2)
        let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
        sim.step();
        let t = sim.cur.get(3, 2);
        assert_eq!(t.instr, expected, "{src:?}");
        assert_eq!(t.tag, 9, "tag always follows the repairer");
        assert_eq!(sim.stats.repairs, 1);
    }
}

#[test]
fn register_repair_is_not_a_replicator() {
    // A row of Repair(E) cells with reg = 0 writes Nop bytes east, not more Repair.
    let (cfg, mut g) = quiet_world(RepairSource::Register, false);
    for x in 0..3 {
        let c = g.get_mut(x, 3);
        c.instr = Instruction::Repair(E).encode();
        c.energy = 500;
    }
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    sim.step();
    assert_eq!(sim.cur.get(3, 3).instr, Instruction::Nop.encode());
    // Under copy-self the same row would have extended itself.
    let (cfg, mut g) = quiet_world(RepairSource::CopySelf, false);
    for x in 0..3 {
        let c = g.get_mut(x, 3);
        c.instr = Instruction::Repair(E).encode();
        c.energy = 500;
    }
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    sim.step();
    assert_eq!(sim.cur.get(3, 3).instr, Instruction::Repair(E).encode());
}

#[test]
fn load_then_repair_loop_maintains_a_heterogeneous_template() {
    // Cell A at (2,2). Its neighbourhood program (ip order N, NE, E, ...): N holds
    // Load(W), NE holds Repair(E). Each sweep A loads the template from W=(1,2) and
    // writes it into E=(3,2). Zapping (3,2) is undone within one sweep; zapping the
    // template (1,2) propagates — information flows from template to copy only.
    let (cfg, mut g) = quiet_world(RepairSource::Register, false);
    g.get_mut(2, 1).instr = Instruction::Load(W).encode(); // N of A
    g.get_mut(3, 1).instr = Instruction::Repair(E).encode(); // NE of A
    g.get_mut(1, 2).instr = 0x77; // template (decodes to Absorb(NW); any byte works)
    g.get_mut(2, 2).ip = 1; // start A at N
    g.get_mut(2, 2).energy = 500;
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    sim.run(2); // Load then Repair
    assert_eq!(sim.cur.get(3, 2).instr, 0x77);
    sim.run(7); // rest of the sweep, back to N
    sim.cur.get_mut(3, 2).instr = 0x11; // zap the copy
    sim.run(9);
    assert_eq!(sim.cur.get(3, 2).instr, 0x77, "copy restored from template");
    sim.cur.get_mut(1, 2).instr = 0x22; // zap the template
    sim.run(9);
    assert_eq!(sim.cur.get(3, 2).instr, 0x22, "template mutation propagates to copy");
}

#[test]
fn no_self_jump_breaks_the_move_ip_trap() {
    // A's east neighbour holds MoveIp(E): with the plan's rule A executes it, jumps to
    // E, executes it again, forever. With no_self_jump A advances past it.
    for (flag, stuck) in [(false, true), (true, false)] {
        let (cfg, mut g) = quiet_world(RepairSource::CopySelf, flag);
        g.get_mut(3, 2).instr = Instruction::MoveIp(E).encode();
        g.get_mut(2, 2).ip = 3; // A starts on its E slot
        g.get_mut(2, 2).energy = 500;
        let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
        let mut ips = Vec::new();
        for _ in 0..12 {
            sim.step();
            ips.push(sim.cur.get(2, 2).ip);
        }
        if stuck {
            assert!(ips.iter().all(|&p| p == 3), "{ips:?}");
        } else {
            assert!(ips.iter().any(|&p| p != 3), "{ips:?}");
            // Every slot gets visited: the sweep continues.
            assert!(ips.contains(&0) && ips.contains(&8), "{ips:?}");
        }
    }
    // A jump to a *different* slot is unaffected by the rule.
    let (cfg, mut g) = quiet_world(RepairSource::CopySelf, true);
    g.get_mut(2, 1).instr = Instruction::MoveIp(NE).encode(); // at N: jump to NE
    g.get_mut(2, 2).ip = N + 1;
    g.get_mut(2, 2).energy = 500;
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    sim.step();
    assert_eq!(sim.cur.get(2, 2).ip, NE + 1);
}

#[test]
fn closed_loop_template_tiling_is_a_fixed_point_only_under_register_repair() {
    // Full-width band: every column is a torus-spanning loop of alternating
    // Repair(S)/Load(N) rows with no open edge. Under register repair each cell keeps
    // rewriting its south neighbour from its north neighbour's byte and the pattern is
    // exactly stable; under copy-self the same seed destroys itself.
    let intact_after = |src: RepairSource, ticks: u32| {
        let cfg = SimConfig {
            width: 16,
            height: 16,
            sun: 3.0,
            sun_profile: SunProfile::Uniform,
            noise_rate: 0.0,
            repair_source: src,
            seed_tiling: true,
            seed_tiling_width: 16,
            ..SimConfig::default()
        };
        let sim0 = Sim::new(cfg.clone(), 3).unwrap();
        let reference = sim0.cur.clone();
        let mut sim = Sim::new(cfg, 3).unwrap();
        sim.run(ticks);
        let same = sim
            .cur
            .cells
            .iter()
            .zip(&reference.cells)
            .filter(|(a, b)| a.instr == b.instr)
            .count();
        (same as f64 / reference.cells.len() as f64, sim.stats.repairs, sim.stats.deaths)
    };
    let (frac, repairs, deaths) = intact_after(RepairSource::Register, 3000);
    assert_eq!(frac, 1.0, "register tiling should be exactly stable");
    assert!(repairs > 0 && deaths == 0);
    let (frac, _, _) = intact_after(RepairSource::CopySelf, 3000);
    assert!(frac < 0.6, "copy-self should not preserve the tiling, intact {frac}");
}

#[test]
fn opposite_repair_relays_the_byte_behind_and_none_writes_nothing() {
    use autopoiesis::grid::W;
    // A at (2,2) executes Repair(E) at ip 0. Under `opposite` the byte written east is
    // the west neighbour's; under `none` nothing is written and no repair is logged.
    let (cfg, mut g) = quiet_world(RepairSource::Opposite, false);
    {
        let c = g.get_mut(2, 2);
        c.instr = Instruction::Repair(E).encode();
        c.reg = 0xAB;
        c.energy = 500;
    }
    g.get_mut(1, 2).instr = 0x5A;
    let mut sim = Sim::with_grid(cfg.clone(), 1, g.clone()).unwrap();
    sim.step();
    assert_eq!(sim.cur.get(3, 2).instr, 0x5A, "opposite relays W into E");
    assert_eq!(sim.stats.repairs, 1);
    let _ = W;

    let (cfg, _) = quiet_world(RepairSource::None, false);
    let mut sim = Sim::with_grid(cfg, 1, g).unwrap();
    let before = sim.cur.get(3, 2).instr;
    sim.step();
    assert_eq!(sim.cur.get(3, 2).instr, before, "none writes nothing");
    assert_eq!(sim.stats.repairs, 0);
    assert!(sim.repair_edges().is_empty());
    // …but it still costs the Repair price.
    assert_eq!(sim.cur.get(2, 2).energy, 500 - 4);
}

#[test]
fn pass_through_tiling_is_a_fixed_point_only_under_opposite_repair() {
    use autopoiesis::config::TilingPattern;
    let intact_after = |src: RepairSource, ticks: u32| {
        let cfg = SimConfig {
            width: 16,
            height: 16,
            sun: 4.0,
            sun_profile: SunProfile::Uniform,
            noise_rate: 0.0,
            repair_source: src,
            seed_tiling: true,
            seed_tiling_width: 16,
            seed_tiling_pattern: TilingPattern::PassThrough,
            ..SimConfig::default()
        };
        let reference = Sim::new(cfg.clone(), 3).unwrap().cur.clone();
        let mut sim = Sim::new(cfg, 3).unwrap();
        // Check two consecutive ticks: under copy-self the tiling *blinks* (every cell
        // is overwritten by the other class each tick), so a single even tick matches.
        sim.run(ticks);
        let frac = |g: &autopoiesis::grid::Grid| {
            g.cells.iter().zip(&reference.cells).filter(|(a, b)| a.instr == b.instr).count() as f64 / reference.cells.len() as f64
        };
        let f0 = frac(&sim.cur);
        sim.step();
        let f1 = frac(&sim.cur);
        (f0.min(f1), sim.stats.repairs)
    };
    let (frac, repairs) = intact_after(RepairSource::Opposite, 3000);
    assert_eq!(frac, 1.0, "pass-through tiling should be exactly stable");
    assert!(repairs > 0);
    let (frac, _) = intact_after(RepairSource::CopySelf, 3000);
    assert!(frac < 0.8, "copy-self should not preserve the tiling, intact {frac}");
}

#[test]
fn probe_records_restoration_for_organism_and_matched_background() {
    use autopoiesis::grid::Grid;
    use autopoiesis::probe::Prober;
    let cfg = SimConfig {
        width: 8,
        height: 8,
        window: 100,
        probe_every: 100,
        probe_k: 4,
        probe_min_size: 4,
        ..SimConfig::default()
    };
    let mut g = Grid::new(8, 8);
    for (i, c) in g.cells.iter_mut().enumerate() {
        c.instr = i as u8;
        c.energy = 10;
    }
    let original = g.clone();
    let core: Vec<u32> = (0..4).map(|y| g.idx(3, y) as u32).collect(); // top half of column 3
    let mut p = Prober::new(&cfg, 7);
    p.perturb(100, &mut g, &[(42, core.clone())]);
    assert_eq!(p.pending(), 1);
    let changed_core = core.iter().filter(|&&c| g.cells[c as usize].instr != original.cells[c as usize].instr).count();
    assert!(changed_core >= 3, "probe should overwrite ~k core bytes (got {changed_core}; a random byte can coincide)");
    // Background cells: in column 3 (same sunlight) but outside the core.
    let changed_bg: Vec<usize> = (0..64)
        .filter(|&i| !core.contains(&(i as u32)) && g.cells[i].instr != original.cells[i].instr)
        .collect();
    assert!(!changed_bg.is_empty() && changed_bg.iter().all(|&i| i % 8 == 3 && i / 8 >= 4), "{changed_bg:?}");
    // Restore the organism fully, leave the background broken: 1.0 vs 0.0.
    for &c in &core {
        g.cells[c as usize].instr = original.cells[c as usize].instr;
    }
    assert!(p.check(150, &g).is_empty(), "no check due before one window");
    assert!(p.check(200, &g).is_empty(), "record only completes after the 5-window check");
    let recs = p.check(600, &g);
    assert_eq!(recs.len(), 1);
    let r = &recs[0];
    assert_eq!((r.organism_id, r.core_size, r.tick), (42, 4, 100));
    assert_eq!(r.restored, [1.0, 1.0, 1.0]);
    assert!(r.bg_k > 0 && r.bg_restored.iter().all(|&v| v == 0.0));
    assert_eq!(p.pending(), 0);
}
