//! Per-cell interpreter. One tick = one instruction per living cell.
//!
//! # Execution model
//!
//! A cell's *program is its neighbourhood*. `ip` (0..9) indexes the 9 cells of the
//! Moore neighbourhood (0 = self, 1..=8 = N, NE, E, SE, S, SW, W, NW). Each tick a cell
//! fetches the `instr` byte held by the cell at `ip` and executes it **as itself**:
//! energy is debited from the executing cell, `reg`/`tag`/`instr` refer to the
//! executing cell, and direction operands are relative to the executing cell. After a
//! normal instruction `ip` advances by one (wrapping mod 9), so an undisturbed cell
//! sweeps its neighbourhood. `MoveIp(d)` / a taken `JmpIfZero(d)` set `ip` to the
//! neighbour `d` instead, so control can loop within the neighbourhood, and a region
//! whose cells hold the same bytes behaves as one program. `Halt` freezes `ip` (at no
//! cost) while energy is at or below `halt_threshold`. With `cfg.no_self_jump` a jump
//! whose destination is the slot currently being fetched from advances instead, which
//! removes the self-loop trap (a neighbour's `MoveIp` pointing back at itself would
//! otherwise freeze the cell for good).
//!
//! What `Repair(d)` writes is `cfg.repair_source`: the cell's own byte (`copy_self`,
//! the plan's original), its `reg` (`register`), or the byte of the neighbourhood slot
//! executed just before the `Repair` (`previous`, the plan's §8 alternative), the byte
//! of the neighbour opposite to `d` (`opposite`, pass-through), or nothing at all
//! (`none`: the null twin — Repair costs but never writes and logs no edge). The tag
//! is always the repairer's.
//!
//! All reads come from `cur`; all writes go to `next`, which starts as a copy of
//! `cur`. Fields a cell owns (`ip`, `reg`, `tag` via `SetTag`, its own energy debit)
//! are written directly. Writes into *other* cells are handled as follows:
//!
//! * **Instruction writes** (`Store`, `Repair`) are collected into one slot per target
//!   cell and applied after every cell has run. If several cells target the same cell
//!   in one tick the writer with the **highest energy wins; ties go to the lowest cell
//!   index** (cells are processed in index order, so the first writer keeps a tie).
//! * **Energy transfers** (`Absorb`, `Share`) are applied immediately against `next`
//!   in cell-index order, clamped to what the source currently holds and to the cap on
//!   the receiver. Energy is never created; clamping at the cap destroys it.
//!
//! **Death.** A cell that cannot afford its instruction burns what it has (energy → 0).
//! After every cell has run, and *before* this tick's energy injection, any cell at 0
//! energy — starved, or drained by an `Absorb` — decoheres: its `instr` is replaced by
//! a random byte and its `tag`/`ip` reset. Pending instruction writes are applied after
//! that, so repairing into a dead cell colonises it (this is how a pattern grows into
//! empty space). A cell that still has 0 energy at the start of a tick (no sun) simply
//! does not execute.

use crate::config::{ExecModel, RepairSource, SimConfig};
use crate::grid::{Cell, Grid, NEIGHBORHOOD, TOKEN, Topology};
use crate::isa::Instruction;
use rand::RngExt;

/// Pending instruction write into a target cell (see module docs for conflicts).
#[derive(Clone, Copy, Default)]
struct WriteSlot {
    set: bool,
    energy: u16,
    instr: u8,
    tag: Option<u8>,
}

/// Counters for one tick.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StepStats {
    pub executed: u64,
    pub deaths: u64,
    pub starved: u64,
    pub repairs: u64,
    pub write_conflicts: u64,
    /// Token model: tokens destroyed this tick (collision, dead destination, starvation).
    pub tokens_lost: u64,
    /// Token model: tokens present after this tick.
    pub tokens: u64,
}

pub struct Vm {
    slots: Vec<WriteSlot>,
    moves: Vec<(u32, u8)>,
}

impl Vm {
    pub fn new(n_cells: usize) -> Vm {
        Vm {
            slots: vec![WriteSlot::default(); n_cells],
            moves: Vec::new(),
        }
    }

    /// Dispatch on `cfg.exec_model`.
    pub fn step<R: RngExt>(
        &mut self,
        cfg: &SimConfig,
        topo: &Topology,
        cur: &Grid,
        next: &mut Grid,
        rng: &mut R,
        repairs: &mut Vec<(u32, u8)>,
    ) -> StepStats {
        match cfg.exec_model {
            ExecModel::Neighbourhood => self.step_neighbourhood(cfg, topo, cur, next, rng, repairs),
            ExecModel::Token => self.step_token(cfg, topo, cur, next, rng, repairs),
        }
    }

    /// Token model (see `ExecModel::Token`). Only cells holding a token run, each its
    /// own byte; then tokens move along the cells' outgoing directions (`MoveIp` /
    /// `JmpIfZero` redirect one shot), applied after every cell has run so that a token
    /// may enter a cell whose own token left this tick; two tokens arriving at one cell,
    /// or a token entering a dead cell, are destroyed (first in index order survives).
    /// `reg` travels with the token. Finally tokens spawn spontaneously at
    /// `token_rate` on live, empty cells.
    pub fn step_token<R: RngExt>(
        &mut self,
        cfg: &SimConfig,
        topo: &Topology,
        cur: &Grid,
        next: &mut Grid,
        rng: &mut R,
        repairs: &mut Vec<(u32, u8)>,
    ) -> StepStats {
        debug_assert_eq!(cur.len(), next.len());
        let n = cur.len();
        let mut stats = StepStats::default();
        for s in &mut self.slots {
            s.set = false;
        }
        self.moves.clear();
        let cap = cfg.energy_cap;
        let costs = &cfg.costs;

        for i in 0..n {
            let c: Cell = cur.cells[i];
            if c.ip & TOKEN == 0 {
                continue;
            }
            if c.energy == 0 {
                // A token on a dead cell (only possible in a hand-built grid) is lost.
                next.cells[i].ip &= !TOKEN;
                stats.tokens_lost += 1;
                continue;
            }
            let instr = Instruction::decode(c.instr);
            let cost = instr.cost(costs);
            if c.energy < cost {
                next.cells[i].energy = 0;
                next.cells[i].ip &= !TOKEN;
                stats.starved += 1;
                stats.tokens_lost += 1;
                continue;
            }
            next.cells[i].energy = next.cells[i].energy.saturating_sub(cost);
            stats.executed += 1;
            let dir = c.ip & 7;
            let mut dest_dir = dir;
            let mut stay = false;

            match instr {
                Instruction::Nop => {}
                Instruction::MoveIp(d) => dest_dir = d & 7,
                Instruction::Load(d) => {
                    next.cells[i].reg = cur.cells[topo.neighbor(i, d)].instr;
                }
                Instruction::Store(d) => {
                    let t = topo.neighbor(i, d);
                    self.propose_write(t, c.energy, c.reg, None, &mut stats);
                }
                Instruction::Repair(d) => {
                    let t = topo.neighbor(i, d);
                    let byte = match cfg.repair_source {
                        RepairSource::CopySelf => Some(c.instr),
                        RepairSource::Register => Some(c.reg),
                        // "Behind in the ip chain": the cell the token came from if the
                        // path is straight, i.e. opposite to the outgoing direction.
                        RepairSource::Previous => Some(cur.cells[topo.neighbor(i, (dir + 4) & 7)].instr),
                        RepairSource::Opposite => Some(cur.cells[topo.neighbor(i, (d + 4) & 7)].instr),
                        RepairSource::None => None,
                    };
                    if let Some(byte) = byte {
                        self.propose_write(t, c.energy, byte, Some(c.tag), &mut stats);
                        repairs.push((i as u32, d & 7));
                        stats.repairs += 1;
                    }
                }
                Instruction::Cmp(d) => {
                    next.cells[i].reg = (cur.cells[topo.neighbor(i, d)].instr == c.instr) as u8;
                }
                Instruction::JmpIfZero(d) => {
                    if c.reg == 0 {
                        dest_dir = d & 7;
                    }
                }
                Instruction::Absorb(d) => {
                    let j = topo.neighbor(i, d);
                    let amt = cfg.absorb_rate.min(next.cells[j].energy);
                    next.cells[j].energy -= amt;
                    next.cells[i].energy = cap.min(next.cells[i].energy.saturating_add(amt));
                }
                Instruction::Share(d) => {
                    let j = topo.neighbor(i, d);
                    let amt = cfg.share_rate.min(next.cells[i].energy);
                    next.cells[i].energy -= amt;
                    next.cells[j].energy = cap.min(next.cells[j].energy.saturating_add(amt));
                }
                Instruction::SetTag => next.cells[i].tag = c.reg,
                Instruction::Halt => {
                    if c.energy <= cfg.halt_threshold {
                        stay = true;
                    }
                }
            }
            if !stay {
                next.cells[i].ip &= !TOKEN;
                self.moves.push((topo.neighbor(i, dest_dir) as u32, next.cells[i].reg));
            }
        }

        for &(j, reg) in &self.moves {
            let nc = &mut next.cells[j as usize];
            if nc.ip & TOKEN == 0 && nc.energy > 0 {
                nc.ip |= TOKEN;
                nc.reg = reg;
            } else {
                stats.tokens_lost += 1;
            }
        }

        // Spontaneous tokens (geometric skipping, as for noise).
        let rate = cfg.token_rate;
        if rate > 0.0 {
            let log_q = (1.0 - rate.min(0.999_999)).ln();
            let mut i = geometric_skip(rng, log_q);
            while i < n {
                let nc = &mut next.cells[i];
                if nc.ip & TOKEN == 0 && nc.energy > 0 {
                    nc.ip |= TOKEN;
                }
                i += 1 + geometric_skip(rng, log_q);
            }
        }

        self.finish(cur, next, rng, &mut stats, true);
        stats.tokens = next.cells.iter().filter(|c| c.ip & TOKEN != 0).count() as u64;
        stats
    }

    /// Neighbourhood model: execute one instruction for every cell. `next` must
    /// already be a copy of `cur`. Every executed `Repair` (winner or not) is appended
    /// to `repairs` as `(source, direction)`.
    pub fn step_neighbourhood<R: RngExt>(
        &mut self,
        cfg: &SimConfig,
        topo: &Topology,
        cur: &Grid,
        next: &mut Grid,
        rng: &mut R,
        repairs: &mut Vec<(u32, u8)>,
    ) -> StepStats {
        debug_assert_eq!(cur.len(), next.len());
        let n = cur.len();
        let mut stats = StepStats::default();
        for s in &mut self.slots {
            s.set = false;
        }
        let cap = cfg.energy_cap;
        let costs = &cfg.costs;

        for i in 0..n {
            let c: Cell = cur.cells[i];
            if c.energy == 0 {
                // Already dead (decohered at the end of the tick it died); nothing to run.
                continue;
            }
            let ip = c.ip % NEIGHBORHOOD;
            let src = if ip == 0 { i } else { topo.neighbor(i, ip - 1) };
            let instr = Instruction::decode(cur.cells[src].instr);
            let cost = instr.cost(costs);
            if c.energy < cost {
                next.cells[i].energy = 0;
                stats.starved += 1;
                continue;
            }
            next.cells[i].energy = next.cells[i].energy.saturating_sub(cost);
            stats.executed += 1;
            let mut new_ip = (ip + 1) % NEIGHBORHOOD;

            match instr {
                Instruction::Nop => {}
                Instruction::MoveIp(d) => new_ip = jump(cfg, ip, d),
                Instruction::Load(d) => {
                    next.cells[i].reg = cur.cells[topo.neighbor(i, d)].instr;
                }
                Instruction::Store(d) => {
                    let t = topo.neighbor(i, d);
                    self.propose_write(t, c.energy, c.reg, None, &mut stats);
                }
                Instruction::Repair(d) => {
                    let t = topo.neighbor(i, d);
                    let byte = match cfg.repair_source {
                        RepairSource::CopySelf => Some(c.instr),
                        RepairSource::Register => Some(c.reg),
                        RepairSource::Previous => {
                            let prev = (ip + NEIGHBORHOOD - 1) % NEIGHBORHOOD;
                            let p = if prev == 0 { i } else { topo.neighbor(i, prev - 1) };
                            Some(cur.cells[p].instr)
                        }
                        RepairSource::Opposite => Some(cur.cells[topo.neighbor(i, (d + 4) & 7)].instr),
                        RepairSource::None => None,
                    };
                    if let Some(byte) = byte {
                        self.propose_write(t, c.energy, byte, Some(c.tag), &mut stats);
                        repairs.push((i as u32, d & 7));
                        stats.repairs += 1;
                    }
                }
                Instruction::Cmp(d) => {
                    next.cells[i].reg = (cur.cells[topo.neighbor(i, d)].instr == c.instr) as u8;
                }
                Instruction::JmpIfZero(d) => {
                    if c.reg == 0 {
                        new_ip = jump(cfg, ip, d);
                    }
                }
                Instruction::Absorb(d) => {
                    let j = topo.neighbor(i, d);
                    let amt = cfg.absorb_rate.min(next.cells[j].energy);
                    next.cells[j].energy -= amt;
                    next.cells[i].energy = cap.min(next.cells[i].energy.saturating_add(amt));
                }
                Instruction::Share(d) => {
                    let j = topo.neighbor(i, d);
                    let amt = cfg.share_rate.min(next.cells[i].energy);
                    next.cells[i].energy -= amt;
                    next.cells[j].energy = cap.min(next.cells[j].energy.saturating_add(amt));
                }
                Instruction::SetTag => next.cells[i].tag = c.reg,
                Instruction::Halt => {
                    if c.energy <= cfg.halt_threshold {
                        new_ip = ip;
                    }
                }
            }
            next.cells[i].ip = new_ip;
        }

        self.finish(cur, next, rng, &mut stats, false);
        stats
    }

    /// Death (decoherence of anything that reached 0 energy this tick) followed by the
    /// pending instruction writes. In the token model a dead cell also gets a random
    /// outgoing direction and loses its token.
    fn finish<R: RngExt>(&mut self, cur: &Grid, next: &mut Grid, rng: &mut R, stats: &mut StepStats, token_model: bool) {
        for (i, nc) in next.cells.iter_mut().enumerate() {
            if nc.energy == 0 && cur.cells[i].energy != 0 {
                nc.instr = rng.random::<u8>();
                nc.tag = 0;
                nc.ip = if token_model { rng.random::<u8>() & 7 } else { 0 };
                stats.deaths += 1;
            }
        }
        for (t, slot) in self.slots.iter().enumerate() {
            if slot.set {
                let nc = &mut next.cells[t];
                nc.instr = slot.instr;
                if let Some(tag) = slot.tag {
                    nc.tag = tag;
                }
            }
        }
    }

    #[inline]
    fn propose_write(&mut self, target: usize, energy: u16, instr: u8, tag: Option<u8>, stats: &mut StepStats) {
        let slot = &mut self.slots[target];
        if slot.set {
            stats.write_conflicts += 1;
            // Strictly greater: on a tie the earlier (lower-index) writer stays.
            if energy <= slot.energy {
                return;
            }
        }
        *slot = WriteSlot {
            set: true,
            energy,
            instr,
            tag,
        };
    }
}

/// Destination ip for a jump to neighbour `d` from current ip `ip`. With
/// `no_self_jump`, jumping back onto the slot currently being fetched from (the
/// self-loop trap) advances instead.
#[inline]
fn jump(cfg: &SimConfig, ip: u8, d: u8) -> u8 {
    let target = (d & 7) + 1;
    if cfg.no_self_jump && target == ip {
        (ip + 1) % NEIGHBORHOOD
    } else {
        target
    }
}

/// Number of cells to skip before the next event at per-cell probability `p`, where
/// `log_q = ln(1 - p)`.
#[inline]
fn geometric_skip<R: RngExt>(rng: &mut R, log_q: f64) -> usize {
    let u: f64 = rng.random::<f64>();
    if u <= 0.0 {
        return usize::MAX / 4;
    }
    let g = (u.ln() / log_q).floor();
    if g >= (usize::MAX / 4) as f64 { usize::MAX / 4 } else { g as usize }
}
