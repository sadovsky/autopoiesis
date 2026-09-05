//! The same envelopes, over real sockets.
//!
//! In `--transport sim` the whole population lives in one process. Over TCP each process
//! holds a *deme* — a sub-population — and the transport routes an envelope to whichever
//! process owns the recipient. Ownership is arithmetic, not a lookup: node ids are
//! allocated in stripes of `ID_STRIDE`, so `to / ID_STRIDE` is the process that owns the
//! node, and no registry, handshake or coordinator is needed to route a gene across a
//! machine boundary.
//!
//! None of this is deterministic, and it is not meant to be: two processes interleave
//! however the kernel decides. `sim` is where the numbers come from; this is the thing
//! that shows a gene actually crossing a network. A run here is watchable with
//! `nc -l` on a spare port, because a frame is one line of JSON.

use crate::gene::NodeId;
use crate::protocol::Envelope;
use crate::transport::Transport;
use anyhow::{Context, Result};
use std::collections::BTreeMap;
use std::io::{BufRead, BufReader, Write};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Node ids per process. The owning process of a node is `id / ID_STRIDE`.
pub const ID_STRIDE: NodeId = 1 << 24;

/// The first node id belonging to process `index`.
pub fn id_base(index: u32) -> NodeId {
    index * ID_STRIDE
}

/// The founder ids of process `index`, given the founding population size — every
/// process can work out its peers' founders without asking anyone.
pub fn founder_ids(index: u32, nodes: usize) -> Vec<NodeId> {
    (0..nodes as NodeId).map(|i| id_base(index) + i).collect()
}

pub struct TcpTransport {
    index: u32,
    peers: Vec<SocketAddr>,
    inbox: Arc<Mutex<Vec<Envelope>>>,
    conns: BTreeMap<u32, TcpStream>,
    sent: u64,
    dropped: u64,
    local: u64,
    remote: u64,
}

impl TcpTransport {
    /// Bind the listener and start accepting. Connections to peers are made lazily, on
    /// the first envelope that needs one, so processes can start in any order.
    pub fn bind(index: u32, listen: SocketAddr, peers: Vec<SocketAddr>) -> Result<TcpTransport> {
        let listener = TcpListener::bind(listen)
            .with_context(|| format!("binding {listen}"))?;
        let inbox: Arc<Mutex<Vec<Envelope>>> = Arc::new(Mutex::new(Vec::new()));
        let accept_inbox = Arc::clone(&inbox);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(stream) = stream else { continue };
                let inbox = Arc::clone(&accept_inbox);
                std::thread::spawn(move || read_frames(stream, inbox));
            }
        });
        Ok(TcpTransport {
            index,
            peers,
            inbox,
            conns: BTreeMap::new(),
            sent: 0,
            dropped: 0,
            local: 0,
            remote: 0,
        })
    }

    pub fn owner(&self, id: NodeId) -> u32 {
        id / ID_STRIDE
    }

    /// Envelopes kept inside this process, and envelopes put on a socket.
    pub fn traffic(&self) -> (u64, u64) {
        (self.local, self.remote)
    }

    fn stream(&mut self, owner: u32) -> Option<&mut TcpStream> {
        if !self.conns.contains_key(&owner) {
            let addr = *self.peers.get(owner as usize)?;
            let stream = TcpStream::connect_timeout(&addr, Duration::from_millis(500)).ok()?;
            stream.set_nodelay(true).ok();
            self.conns.insert(owner, stream);
        }
        self.conns.get_mut(&owner)
    }
}

fn read_frames(stream: TcpStream, inbox: Arc<Mutex<Vec<Envelope>>>) {
    let reader = BufReader::new(stream);
    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        // A frame from another process is data, not instruction: anything that does not
        // parse as an envelope is dropped and the connection kept.
        if let Ok(env) = Envelope::decode(&line)
            && let Ok(mut q) = inbox.lock()
        {
            q.push(env);
        }
    }
}

impl Transport for TcpTransport {
    fn send(&mut self, env: Envelope) {
        self.sent += 1;
        let owner = self.owner(env.to);
        if owner == self.index {
            self.local += 1;
            if let Ok(mut q) = self.inbox.lock() {
                q.push(env);
            }
            return;
        }
        let line = env.encode();
        match self.stream(owner) {
            Some(stream) => {
                if stream.write_all(line.as_bytes()).and_then(|_| stream.write_all(b"\n")).is_err() {
                    // The peer went away; drop the connection and this message with it.
                    self.conns.remove(&owner);
                    self.dropped += 1;
                } else {
                    self.remote += 1;
                }
            }
            None => self.dropped += 1,
        }
    }

    fn deliver(&mut self, _tick: u32) -> Vec<Envelope> {
        match self.inbox.lock() {
            Ok(mut q) => std::mem::take(&mut *q),
            Err(_) => Vec::new(),
        }
    }

    fn counts(&self) -> (u64, u64) {
        (self.sent, self.dropped)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gene::{Acquisition, Gene};
    use crate::protocol::Message;
    use std::time::Instant;

    fn local(port: u16) -> SocketAddr {
        format!("127.0.0.1:{port}").parse().expect("valid address")
    }

    #[test]
    fn a_gene_crosses_a_real_socket_between_two_processes_worth_of_nodes() {
        // Two transports, two ports, one gene: the bytes go out through the kernel and
        // come back as the same gene on the other side.
        let (a_addr, b_addr) = (local(19731), local(19732));
        let peers = vec![a_addr, b_addr];
        let mut a = TcpTransport::bind(0, a_addr, peers.clone()).expect("bind a");
        let mut b = TcpTransport::bind(1, b_addr, peers).expect("bind b");

        let gene = Gene::new(vec![0x0a, 0x4d, 0x0d], 0, 3, true);
        let to = id_base(1) + 7;
        let msg = Message::Transfer { strain: 2, gene: gene.clone(), via: Acquisition::Conjugation };
        a.send(Envelope::new(id_base(0) + 1, to, msg));

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut got = Vec::new();
        while got.is_empty() && Instant::now() < deadline {
            got = b.deliver(0);
        }
        assert_eq!(got.len(), 1, "nothing arrived over the socket");
        assert_eq!(got[0].to, to);
        match &got[0].msg {
            Message::Transfer { gene: arrived, strain, .. } => {
                assert_eq!(arrived, &gene, "the gene changed in flight");
                assert_eq!(*strain, 2);
            }
            other => panic!("wrong message arrived: {other:?}"),
        }
        assert_eq!(a.traffic().1, 1, "the envelope should have gone out on a socket");
    }

    #[test]
    fn envelopes_for_this_process_never_touch_the_network() {
        let addr = local(19733);
        let mut t = TcpTransport::bind(0, addr, vec![addr]).expect("bind");
        t.send(Envelope::new(1, 2, Message::Hello { strain: 0, peers: vec![] }));
        assert_eq!(t.traffic(), (1, 0));
        assert_eq!(t.deliver(0).len(), 1);
        assert_eq!(t.counts(), (1, 0));
    }

    #[test]
    fn ownership_is_arithmetic() {
        let addr = local(19734);
        let t = TcpTransport::bind(3, addr, vec![addr]).expect("bind");
        assert_eq!(t.owner(id_base(3) + 5), 3);
        assert_eq!(t.owner(id_base(0)), 0);
        assert_eq!(founder_ids(2, 3), vec![id_base(2), id_base(2) + 1, id_base(2) + 2]);
    }
}
