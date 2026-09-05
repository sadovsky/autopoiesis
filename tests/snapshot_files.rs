use autopoiesis::config::SimConfig;
use autopoiesis::sim::Sim;
use autopoiesis::snapshot::{Snapshot, list_snapshots};

#[test]
fn snapshots_written_to_disk_load_back_losslessly() {
    let dir = std::env::temp_dir().join(format!("autopoiesis-snap-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let cfg = SimConfig {
        width: 20,
        height: 12,
        noise_rate: 0.01,
        ..SimConfig::default()
    };
    let mut sim = Sim::new(cfg, 77).unwrap();
    let mut written = Vec::new();
    for _ in 0..3 {
        sim.run(50);
        let snap = Snapshot {
            tick: sim.tick,
            noise_rate: sim.noise_rate(),
            grid: sim.cur.clone(),
            edges: sim.repair_log.edges(),
        };
        snap.write(&Snapshot::path_for(&dir, sim.tick)).unwrap();
        written.push(snap);
    }
    let listed = list_snapshots(&dir).unwrap();
    assert_eq!(listed.iter().map(|(t, _)| *t).collect::<Vec<_>>(), vec![50, 100, 150]);
    for ((_, path), expected) in listed.iter().zip(&written) {
        let back = Snapshot::read(path).unwrap();
        assert_eq!(&back, expected);
        assert_eq!(back.grid.hash(), expected.grid.hash());
        assert!(!back.edges.is_empty(), "a running grid should have repair edges");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
