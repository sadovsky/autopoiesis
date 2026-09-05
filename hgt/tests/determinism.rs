//! Phase 5-6 acceptance: a run in the simulated transport is fully described by
//! `(config, seed)`, and the metrics are a function of the event stream — so measuring a
//! run live and re-deriving it later from the log are the same computation.

use hgt::config::{HgtConfig, Mechanisms};
use hgt::event::Event;
use hgt::metrics::{Analyzer, Record};
use hgt::world::World;

fn cfg() -> HgtConfig {
    HgtConfig {
        nodes: 32,
        max_nodes: 128,
        epoch_ticks: 120,
        hazard_kinds: 3,
        mechanisms: Mechanisms::default(),
        ..HgtConfig::default()
    }
}

/// Run and return (world hash, the event log as JSON lines, the metric records as JSON).
fn run(cfg: HgtConfig, seed: u64, ticks: u32) -> (u64, Vec<String>, Vec<String>) {
    let mut analyzer = Analyzer::new(&cfg, seed);
    let mut world = World::new(cfg, seed).expect("valid config");
    let (mut log, mut records) = (Vec::new(), Vec::new());
    let feed = |e: &Event, log: &mut Vec<String>, records: &mut Vec<String>, a: &mut Analyzer| {
        log.push(serde_json::to_string(e).expect("events serialize"));
        for rec in a.observe(e) {
            records.push(serde_json::to_string(&rec).expect("records serialize"));
        }
    };
    for e in world.founding_events() {
        feed(&e, &mut log, &mut records, &mut analyzer);
    }
    for _ in 0..ticks {
        for e in world.step() {
            feed(&e, &mut log, &mut records, &mut analyzer);
        }
        if world.extinct() {
            break;
        }
    }
    for rec in analyzer.finish() {
        records.push(serde_json::to_string(&rec).expect("records serialize"));
    }
    (world.hash(), log, records)
}

#[test]
fn one_seed_is_one_run() {
    let (h1, log1, rec1) = run(cfg(), 21, 400);
    let (h2, log2, rec2) = run(cfg(), 21, 400);
    assert_eq!(h1, h2, "same seed, different world");
    assert_eq!(log1, log2, "same seed, different history");
    assert_eq!(rec1, rec2, "same seed, different metrics");

    let (h3, _, rec3) = run(cfg(), 22, 400);
    assert_ne!(h1, h3, "different seeds gave the same world");
    assert_ne!(rec1, rec3);
}

#[test]
fn measuring_live_and_re_deriving_from_the_log_agree_exactly() {
    let seed = 21;
    let (_, log, live) = run(cfg(), seed, 400);

    // Exactly what `hgt analyze` does: parse the log back, feed a fresh analyzer.
    let mut analyzer = Analyzer::new(&cfg(), seed);
    let mut replayed = Vec::new();
    for line in &log {
        let event: Event = serde_json::from_str(line).expect("events round-trip");
        for rec in analyzer.observe(&event) {
            replayed.push(serde_json::to_string(&rec).expect("records serialize"));
        }
    }
    for rec in analyzer.finish() {
        replayed.push(serde_json::to_string(&rec).expect("records serialize"));
    }
    assert_eq!(live.len(), replayed.len(), "online produced {} records, replay {}", live.len(), replayed.len());
    for (i, (a, b)) in live.iter().zip(&replayed).enumerate() {
        assert_eq!(a, b, "record {i} differs between the live run and the replay");
    }
    assert!(!live.is_empty());
}

#[test]
fn a_lossy_network_changes_the_run_without_making_it_unrepeatable() {
    let lossy = HgtConfig { loss: 0.3, latency: 3, ..cfg() };
    let (h1, _, rec1) = run(lossy.clone(), 21, 400);
    let (h2, _, rec2) = run(lossy, 21, 400);
    assert_eq!((h1, &rec1), (h2, &rec2), "loss must be deterministic given the seed");

    let (clean, _, _) = run(cfg(), 21, 400);
    assert_ne!(h1, clean, "dropping a third of the messages changed nothing");
}

#[test]
fn the_metric_records_a_run_emits_are_the_three_kinds_and_nothing_else() {
    let (_, _, records) = run(cfg(), 5, 400);
    let mut kinds: Vec<String> = records
        .iter()
        .map(|r| serde_json::from_str::<serde_json::Value>(r).unwrap()["kind"].as_str().unwrap().to_string())
        .collect();
    kinds.sort();
    kinds.dedup();
    assert_eq!(kinds, vec!["epoch", "frame", "gene"]);
    assert!(matches!(
        serde_json::from_str::<serde_json::Value>(&records[0]).unwrap()["kind"].as_str(),
        Some("frame")
    ));
    // A `Record` is one of three shapes and serializes untagged, so nothing is nested.
    let _: Record = {
        let mut a = Analyzer::new(&cfg(), 5);
        a.observe(&Event::Tick { tick: 0, hazard: 0, alive: 0, survived: 0, failed: 0, energy_total: 0 })
            .pop()
            .expect("a tick on an analysis boundary emits a frame")
    };
}
