//! The substrate: a toroidal grid of small cells and the Moore-neighbourhood topology.

use rand::RngExt;

/// The atomic unit. There is no "organism" type; everything is cells.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
#[repr(C)]
pub struct Cell {
    /// Decodes to an `Instruction`; invalid bytes decode to `Nop`.
    pub instr: u8,
    /// `0..=config.energy_cap`. A cell at 0 is overwritten with a random byte next tick.
    pub energy: u16,
    /// Offset into the cell's 9-cell neighbourhood (0 = self, 1..=8 = compass
    /// directions). The cell executes the instruction held by the cell at `ip`.
    /// Wraps modulo `NEIGHBORHOOD`.
    pub ip: u8,
    /// One general-purpose register.
    pub reg: u8,
    /// Self-marker, propagated by `Repair`; used only for display and boundary metrics.
    pub tag: u8,
}

pub const CELL_BYTES: usize = 6;

impl Cell {
    pub fn to_bytes(&self) -> [u8; CELL_BYTES] {
        let e = self.energy.to_le_bytes();
        [self.instr, e[0], e[1], self.ip, self.reg, self.tag]
    }

    pub fn from_bytes(b: &[u8]) -> Cell {
        Cell {
            instr: b[0],
            energy: u16::from_le_bytes([b[1], b[2]]),
            ip: b[3],
            reg: b[4],
            tag: b[5],
        }
    }
}

/// Compass directions, indexed 0..8: N, NE, E, SE, S, SW, W, NW (clockwise from north,
/// with y growing downward).
pub const DIRS: [(i32, i32); 8] = [
    (0, -1),
    (1, -1),
    (1, 0),
    (1, 1),
    (0, 1),
    (-1, 1),
    (-1, 0),
    (-1, -1),
];

pub const DIR_NAMES: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

/// Arrow glyphs for the renderer, in `DIRS` order.
pub const DIR_ARROWS: [char; 8] = ['↑', '↗', '→', '↘', '↓', '↙', '←', '↖'];

/// Size of the neighbourhood a cell's `ip` ranges over (self + 8 neighbours).
pub const NEIGHBORHOOD: u8 = 9;

/// Token model: this bit of `ip` marks a cell that holds a token; `ip & 7` is then the
/// cell's outgoing direction.
pub const TOKEN: u8 = 0x80;

pub const N: u8 = 0;
pub const NE: u8 = 1;
pub const E: u8 = 2;
pub const SE: u8 = 3;
pub const S: u8 = 4;
pub const SW: u8 = 5;
pub const W: u8 = 6;
pub const NW: u8 = 7;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Grid {
    pub width: usize,
    pub height: usize,
    pub cells: Vec<Cell>,
}

impl Grid {
    pub fn new(width: usize, height: usize) -> Grid {
        Grid {
            width,
            height,
            cells: vec![Cell::default(); width * height],
        }
    }

    /// Random initial state: random instruction byte, energy in `1..=init_energy_max`,
    /// everything else zero. Reproducible given the RNG.
    pub fn random<R: RngExt>(width: usize, height: usize, init_energy_max: u16, rng: &mut R) -> Grid {
        let mut g = Grid::new(width, height);
        for c in &mut g.cells {
            c.instr = rng.random::<u8>();
            c.energy = rng.random_range(1..=init_energy_max);
        }
        g
    }

    #[inline]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Wrapping (x, y) → flat index.
    #[inline]
    pub fn idx(&self, x: i64, y: i64) -> usize {
        let w = self.width as i64;
        let h = self.height as i64;
        let xx = x.rem_euclid(w);
        let yy = y.rem_euclid(h);
        (yy * w + xx) as usize
    }

    #[inline]
    pub fn xy(&self, i: usize) -> (usize, usize) {
        (i % self.width, i / self.width)
    }

    /// Index of the cell at `(dx, dy)` from cell `i`, wrapping.
    #[inline]
    pub fn offset(&self, i: usize, dx: i32, dy: i32) -> usize {
        let (x, y) = self.xy(i);
        self.idx(x as i64 + dx as i64, y as i64 + dy as i64)
    }

    /// Index of the neighbour of `i` in compass direction `dir` (0..8).
    #[inline]
    pub fn neighbor(&self, i: usize, dir: u8) -> usize {
        let (dx, dy) = DIRS[(dir & 7) as usize];
        self.offset(i, dx, dy)
    }

    #[inline]
    pub fn get(&self, x: i64, y: i64) -> &Cell {
        &self.cells[self.idx(x, y)]
    }

    #[inline]
    pub fn get_mut(&mut self, x: i64, y: i64) -> &mut Cell {
        let i = self.idx(x, y);
        &mut self.cells[i]
    }

    pub fn total_energy(&self) -> u64 {
        self.cells.iter().map(|c| c.energy as u64).sum()
    }

    /// FNV-1a over every cell's bytes. Used for the determinism test and run logs.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        for c in &self.cells {
            for b in c.to_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x0000_0100_0000_01b3);
            }
        }
        h
    }
}

/// Precomputed neighbour indices for every cell, in `DIRS` order. Kept outside `Grid`
/// so that cloning a grid (double-buffering, snapshots) does not copy it.
#[derive(Clone, Debug)]
pub struct Topology {
    pub nbrs: Vec<[u32; 8]>,
}

impl Topology {
    pub fn new(width: usize, height: usize) -> Topology {
        let g = Grid::new(width, height);
        let nbrs = (0..g.len())
            .map(|i| {
                let mut n = [0u32; 8];
                for (d, slot) in n.iter_mut().enumerate() {
                    *slot = g.neighbor(i, d as u8) as u32;
                }
                n
            })
            .collect();
        Topology { nbrs }
    }

    #[inline]
    pub fn neighbor(&self, i: usize, dir: u8) -> usize {
        self.nbrs[i][(dir & 7) as usize] as usize
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand_xoshiro::Xoshiro256PlusPlus;

    #[test]
    fn wraps_toroidally() {
        let g = Grid::new(5, 4);
        assert_eq!(g.idx(0, 0), 0);
        assert_eq!(g.idx(-1, 0), 4);
        assert_eq!(g.idx(5, 0), 0);
        assert_eq!(g.idx(0, -1), 15);
        assert_eq!(g.idx(0, 4), 0);
        assert_eq!(g.neighbor(0, NW), 19);
        assert_eq!(g.neighbor(0, E), 1);
        assert_eq!(g.neighbor(0, S), 5);
        let t = Topology::new(5, 4);
        for i in 0..20 {
            for d in 0..8u8 {
                assert_eq!(t.neighbor(i, d), g.neighbor(i, d));
            }
        }
    }

    #[test]
    fn random_init_is_reproducible() {
        let mut r1 = Xoshiro256PlusPlus::seed_from_u64(42);
        let mut r2 = Xoshiro256PlusPlus::seed_from_u64(42);
        let a = Grid::random(16, 16, 100, &mut r1);
        let b = Grid::random(16, 16, 100, &mut r2);
        assert_eq!(a, b);
        assert_eq!(a.hash(), b.hash());
        let mut r3 = Xoshiro256PlusPlus::seed_from_u64(43);
        let c = Grid::random(16, 16, 100, &mut r3);
        assert_ne!(a.hash(), c.hash());
        assert!(a.cells.iter().all(|c| c.energy >= 1 && c.energy <= 100));
    }

    #[test]
    fn cell_bytes_roundtrip() {
        let c = Cell { instr: 0xAB, energy: 0x1234, ip: 7, reg: 9, tag: 200 };
        assert_eq!(Cell::from_bytes(&c.to_bytes()), c);
    }
}
