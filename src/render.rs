//! Terminal renderer: one character per cell.
//!
//! * `tag` mode (default): colour by `tag` (256-colour palette), glyph brightness by
//!   energy. Regions of uniform colour are candidate organisms.
//! * `ip` mode: cells whose own byte is `MoveIp`/`JmpIfZero` are drawn as arrows in
//!   the jump direction so control-flow loops are visible; others as in `tag` mode.
//! * `instr` mode: one-letter opcode mnemonics, coloured by opcode class.
//!
//! Keys: `q`/Esc quit, space pauses.

use crate::grid::{DIR_ARROWS, Grid};
use crate::isa::Instruction;
use crate::sim::Sim;
use anyhow::Result;
use crossterm::{
    cursor,
    event::{self, Event, KeyCode, KeyEvent, KeyModifiers},
    execute, queue,
    style::{Color, Print, ResetColor, SetForegroundColor},
    terminal::{self, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use std::io::{Stdout, Write, stdout};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, Debug, PartialEq, Eq, clap::ValueEnum)]
pub enum ShowMode {
    Tag,
    Ip,
    Instr,
}

pub struct Renderer {
    out: Stdout,
    mode: ShowMode,
    min_frame: Duration,
    last_frame: Option<Instant>,
    buf: String,
}

impl Renderer {
    pub fn new(mode: ShowMode, fps: f64) -> Result<Renderer> {
        let mut out = stdout();
        terminal::enable_raw_mode()?;
        execute!(out, EnterAlternateScreen, cursor::Hide, Clear(ClearType::All))?;
        let min_frame = if fps > 0.0 {
            Duration::from_secs_f64(1.0 / fps)
        } else {
            Duration::ZERO
        };
        Ok(Renderer {
            out,
            mode,
            min_frame,
            last_frame: None,
            buf: String::new(),
        })
    }

    /// Draw one frame. Returns `false` if the user asked to quit.
    pub fn frame(&mut self, sim: &Sim, status: &str) -> Result<bool> {
        if let Some(t) = self.last_frame {
            let el = t.elapsed();
            if el < self.min_frame {
                std::thread::sleep(self.min_frame - el);
            }
        }
        self.last_frame = Some(Instant::now());

        let (cols, rows) = terminal::size()?;
        let grid = &sim.cur;
        let vw = grid.width.min(cols as usize);
        let vh = grid.height.min(rows.saturating_sub(1) as usize);
        let cap = sim.cfg.energy_cap;

        queue!(self.out, cursor::MoveTo(0, 0))?;
        let mut cur_color: Option<Color> = None;
        for y in 0..vh {
            self.buf.clear();
            queue!(self.out, cursor::MoveTo(0, y as u16))?;
            for x in 0..vw {
                let cell = &grid.cells[y * grid.width + x];
                let (ch, color) = paint(self.mode, cell.instr, cell.energy, cell.tag, cap);
                if cur_color != Some(color) {
                    if !self.buf.is_empty() {
                        queue!(self.out, Print(&self.buf))?;
                        self.buf.clear();
                    }
                    queue!(self.out, SetForegroundColor(color))?;
                    cur_color = Some(color);
                }
                self.buf.push(ch);
            }
            queue!(self.out, Print(&self.buf), Clear(ClearType::UntilNewLine))?;
        }
        queue!(
            self.out,
            ResetColor,
            cursor::MoveTo(0, vh as u16),
            Print(status),
            Clear(ClearType::UntilNewLine)
        )?;
        self.out.flush()?;

        // Input: non-blocking poll; space pauses until the next key.
        let mut paused = false;
        loop {
            let timeout = if paused { Duration::from_millis(100) } else { Duration::ZERO };
            if !event::poll(timeout)? {
                if paused {
                    continue;
                }
                break;
            }
            if let Event::Key(KeyEvent { code, modifiers, .. }) = event::read()? {
                match code {
                    KeyCode::Char('q') | KeyCode::Esc => return Ok(false),
                    KeyCode::Char('c') if modifiers.contains(KeyModifiers::CONTROL) => return Ok(false),
                    KeyCode::Char(' ') => paused = !paused,
                    _ => {
                        if paused {
                            paused = false;
                        }
                    }
                }
            }
            if !paused {
                break;
            }
        }
        Ok(true)
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = execute!(self.out, ResetColor, cursor::Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

/// Glyph for an energy level: blank when dead, then four brightness steps.
pub fn energy_glyph(energy: u16, cap: u16) -> char {
    if energy == 0 {
        return ' ';
    }
    let cap = cap.max(1) as u32;
    let q = energy as u32 * 8 / cap;
    match q {
        0 => '·',
        1 => '▪',
        2..=4 => '▓',
        _ => '█',
    }
}

/// 256-colour palette entry for a tag; tag 0 (untagged) is dim grey.
pub fn tag_color(tag: u8) -> Color {
    if tag == 0 {
        Color::DarkGrey
    } else {
        // Spread tags over the 6x6x6 colour cube, skipping the darkest entries.
        Color::AnsiValue(16 + ((tag as u16 * 97 + 40) % 200) as u8 + 16)
    }
}

pub fn instr_color(ins: Instruction, energy: u16) -> Color {
    if energy == 0 {
        return Color::DarkGrey;
    }
    match ins {
        Instruction::Repair(_) => Color::Green,
        Instruction::Absorb(_) => Color::Red,
        Instruction::Share(_) => Color::Blue,
        Instruction::Store(_) | Instruction::Load(_) => Color::Yellow,
        Instruction::MoveIp(_) | Instruction::JmpIfZero(_) => Color::Magenta,
        Instruction::Cmp(_) | Instruction::SetTag => Color::Cyan,
        Instruction::Nop | Instruction::Halt => Color::Grey,
    }
}

/// Character and colour for one cell in the given mode.
pub fn paint(mode: ShowMode, instr: u8, energy: u16, tag: u8, cap: u16) -> (char, Color) {
    let ins = Instruction::decode(instr);
    match mode {
        ShowMode::Tag => (energy_glyph(energy, cap), tag_color(tag)),
        ShowMode::Ip => match ins {
            Instruction::MoveIp(d) | Instruction::JmpIfZero(d) if energy > 0 => {
                (DIR_ARROWS[(d & 7) as usize], instr_color(ins, energy))
            }
            _ => (energy_glyph(energy, cap), tag_color(tag)),
        },
        ShowMode::Instr => {
            let ch = if energy == 0 { ' ' } else { ins.glyph() };
            (ch, instr_color(ins, energy))
        }
    }
}

/// Plain-text dump of a grid (used by `--dump-grid` and debugging): opcode glyphs.
pub fn ascii(grid: &Grid) -> String {
    let mut s = String::with_capacity((grid.width + 1) * grid.height);
    for y in 0..grid.height {
        for x in 0..grid.width {
            let c = &grid.cells[y * grid.width + x];
            s.push(if c.energy == 0 { ' ' } else { Instruction::decode(c.instr).glyph() });
        }
        s.push('\n');
    }
    s
}
