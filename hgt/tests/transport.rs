//! Phase 6 acceptance: the gene goes over a real socket.
//!
//! Two demes, two listeners, one localhost network. The second deme is founded with no
//! spare genes at all, so if it is still alive after the stressor shifts, the genes that
//! saved it arrived as bytes on a TCP connection from the first.

use hgt::config::{HgtConfig, Mechanisms};
use hgt::event::Event;
use hgt::gene::NodeId;
use hgt::tcp::{ID_STRIDE, TcpTransport, founder_ids, id_base};
use hgt::world::World;
use std::net::SocketAddr;
use std::time::Duration;

fn cfg(founder_carriers: usize) -> HgtConfig {
    HgtConfig {
        nodes: 12,
        max_nodes: 48,
        epoch_ticks: 80,
        hazard_kinds: 3,
        founder_carriers,
        restriction: 0.0,
        mechanisms: Mechanisms::default(),
        ..HgtConfig::default()
    }
}

fn deme(index: u32, addrs: &[SocketAddr], carriers: usize, seed: u64) -> World {
    let base = id_base(index);
    let transport = TcpTransport::bind(index, addrs[index as usize], addrs.to_vec())
        .expect("bind a listener");
    let cfg = cfg(carriers);
    let mut w = World::with_deme(cfg.clone(), seed, Box::new(transport), base..base + ID_STRIDE)
        .expect("valid config");
    let remote: Vec<NodeId> = (0..addrs.len() as u32)
        .filter(|i| *i != index)
        .flat_map(|i| founder_ids(i, cfg.nodes))
        .collect();
    w.introduce(&remote, 2);
    w
}

#[test]
fn a_deme_with_no_spare_genes_survives_on_what_arrives_over_the_wire() {
    let addrs: Vec<SocketAddr> = ["127.0.0.1:19751", "127.0.0.1:19752"]
        .iter()
        .map(|s| s.parse().expect("valid address"))
        .collect();
    let seed = 4;
    let mut rich = deme(0, &addrs, 2, seed);
    let mut poor = deme(1, &addrs, 0, seed);

    let mut crossed = 0;
    for _ in 0..300 {
        rich.step();
        for e in poor.step() {
            if let Event::Acquire { from: Some(donor), node, .. } = e
                && donor / ID_STRIDE != node / ID_STRIDE
            {
                crossed += 1;
            }
        }
        // Real sockets need a moment; the reader threads are what make deliveries visible.
        std::thread::sleep(Duration::from_millis(1));
    }

    assert!(crossed > 0, "nothing crossed the socket into the second deme");
    assert!(poor.population() > 0, "the deme with no spare genes died anyway");
    assert!(rich.population() > 0);

    // The control: the same deme, alone in the world, with nobody to receive from.
    let alone_addr: Vec<SocketAddr> = vec!["127.0.0.1:19753".parse().expect("valid address")];
    let mut alone = deme(0, &alone_addr, 0, seed);
    for _ in 0..300 {
        alone.step();
        if alone.extinct() {
            break;
        }
    }
    assert!(alone.extinct(), "a deme with no spares and no peers should not survive the shifts");
}
