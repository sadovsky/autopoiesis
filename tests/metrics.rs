//! Phase 5 acceptance: a hand-built self-repairing ring yields one SCC with
//! persistence > 5; a random grid yields none (and a fake cycle on a random grid has
//! persistence ≈ 1, so the SCC alone is not "life").

use autopoiesis::config::{SimConfig, SunProfile};
use autopoiesis::grid::{Grid, S};
use autopoiesis::isa::Instruction;
use autopoiesis::metrics::{Analyzer, find_organisms};
use autopoiesis::sim::Sim;
use rand::SeedableRng;
use rand_xoshiro::Xoshiro256PlusPlus;

fn cfg(w: usize, h: usize) -> SimConfig {
    SimConfig {
        width: w,
        height: h,
        window: 100,
        mi_lag: 100,
        analysis_every: 20,
        min_size: 3,
        mi_samples: 8,
        ..SimConfig::default()
    }
}

/// Column `x` of `Repair(S)`: a torus-spanning ring where each cell repairs the next.
fn ring_edges(g: &Grid, x: i64) -> Vec<(u32, u32)> {
    (0..g.height as i64)
        .map(|y| (g.idx(x, y) as u32, g.idx(x, y + 1) as u32))
        .collect()
}

#[test]
fn synthetic_ring_is_one_persistent_organism_and_random_grid_is_none() {
    let (w, h) = (24, 24);
    let c = cfg(w, h);
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(11);
    let ring_x = 10;
    let ring_byte = Instruction::Repair(S).encode();

    // --- Ring world: the ring column is fixed, everything else is re-randomised
    // every frame (a fully decorrelating background).
    let mut an = Analyzer::new(&c, 1);
    let mut last = None;
    for k in 0..=15u32 {
        let tick = k * 20;
        let mut g = Grid::random(w, h, 100, &mut rng);
        for y in 0..h as i64 {
            g.get_mut(ring_x, y).instr = ring_byte;
        }
        let edges = ring_edges(&g, ring_x);
        let rep = an.observe(tick, 0.001, &g, &edges);
        assert_eq!(rep.frame.n_organisms, 1, "tick {tick}");
        assert_eq!(rep.frame.sizes, vec![h]);
        assert!(rep.deaths.is_empty());
        last = Some(rep.frame);
    }
    let frame = last.unwrap();
    let org = &frame.organisms[0];
    assert_eq!(org.id, 0, "one organism tracked throughout");
    assert!(org.mi_samples > 0);
    assert!(
        org.persistence > 5.0,
        "ring persistence {} (mi_region {}, mi_random {})",
        org.persistence,
        org.mi_region,
        org.mi_random
    );
    assert!(org.stability > 0.3 && org.stability_random < 0.05);
    let survivors = an.finish();
    assert_eq!(survivors.len(), 1);
    assert_eq!(survivors[0].died, None);
    assert_eq!(survivors[0].max_size, h);

    // --- Random world, no repair edges: nothing.
    let mut an = Analyzer::new(&c, 2);
    for k in 0..=10u32 {
        let g = Grid::random(w, h, 100, &mut rng);
        let rep = an.observe(k * 20, 0.001, &g, &[]);
        assert_eq!(rep.frame.n_organisms, 0);
    }
    assert!(an.finish().is_empty());

    // --- Random world with a fabricated repair cycle: an SCC exists but it preserves
    // nothing, so persistence sits near 1.
    let mut an = Analyzer::new(&c, 3);
    let mut last = None;
    for k in 0..=15u32 {
        let g = Grid::random(w, h, 100, &mut rng);
        let edges = ring_edges(&g, ring_x);
        let rep = an.observe(k * 20, 0.001, &g, &edges);
        assert_eq!(rep.frame.n_organisms, 1);
        last = Some(rep.frame);
    }
    let p = last.unwrap().organisms[0].persistence;
    assert!(p < 2.0, "fake-cycle persistence {p}");
}

#[test]
fn organism_death_is_recorded_with_vitality() {
    let (w, h) = (16, 16);
    let c = SimConfig {
        window: 100,
        ..cfg(w, h)
    };
    let mut rng = Xoshiro256PlusPlus::seed_from_u64(5);
    let mut an = Analyzer::new(&c, 9);
    let g = Grid::random(w, h, 100, &mut rng);
    let edges = ring_edges(&g, 3);
    // Present for ticks 0..=200, gone from 220 on; noise ramps 0.01 per frame.
    for k in 0..=20u32 {
        let tick = k * 20;
        let noise = 0.001 * k as f64;
        let e = if tick <= 200 { edges.clone() } else { Vec::new() };
        let rep = an.observe(tick, noise, &g, &e);
        if tick < 220 + 100 {
            assert!(rep.deaths.is_empty(), "premature death at {tick}");
        } else if tick == 320 {
            assert_eq!(rep.deaths.len(), 1);
            let d = &rep.deaths[0];
            assert_eq!(d.born, 0);
            assert_eq!(d.died, Some(220));
            assert_eq!(d.max_size, h);
            assert!((d.vitality.unwrap() - 0.011).abs() < 1e-9);
        }
    }
    assert!(an.finish().is_empty());
}

#[test]
fn seeded_band_in_a_live_sim_is_detected_and_measured() {
    // End-to-end wiring check on a live sim: a 3-wide band of Repair(S) is injected.
    // Its columns are torus-spanning repair cycles and copy-self Repair spreads the
    // band outward, so between ticks 100 and 200 there is always at least one SCC with
    // a full column (24 cells) in its core, with MI samples pooled and a finite
    // persistence. (Whether the band is *more* persistent than its surroundings is an
    // experimental question — see results/ — not an invariant: in this substrate the
    // bright side becomes one churning Repair soup and frozen background looks
    // persistent too.)
    let c = SimConfig {
        sun: 4.0,
        sun_profile: SunProfile::Uniform,
        noise_rate: 0.001,
        seed_ring: true,
        seed_ring_x: Some(12),
        seed_ring_width: 3,
        ..cfg(24, 24)
    };
    let mut sim = Sim::new(c.clone(), 4).unwrap();
    let mut an = Analyzer::new(&c, 4);
    let mut frames_checked = 0;
    while sim.tick <= 200 {
        if sim.tick % c.analysis_every == 0 {
            let edges = sim.repair_edges();
            let rep = an.observe(sim.tick, sim.noise_rate(), &sim.cur, &edges);
            if sim.tick >= 100 {
                frames_checked += 1;
                let big = rep
                    .frame
                    .organisms
                    .iter()
                    .find(|o| o.core_size >= 24)
                    .unwrap_or_else(|| panic!("tick {}: no column-sized SCC; sizes {:?}", sim.tick, rep.frame.sizes));
                assert!(big.mi_samples > 0);
                assert!(big.persistence.is_finite() && big.persistence >= 0.0);
                assert!(big.stability >= 0.0 && big.stability <= 1.0);
                assert_eq!(rep.frame.sizes.len(), rep.frame.n_organisms);
                assert!(rep.frame.background_stability > 0.0);
            }
        }
        sim.step();
    }
    assert_eq!(frames_checked, 6);
    let lives = an.finish();
    assert!(!lives.is_empty());
    assert!(lives.iter().any(|l| l.max_size >= 24));
}
