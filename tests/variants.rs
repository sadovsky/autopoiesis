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
