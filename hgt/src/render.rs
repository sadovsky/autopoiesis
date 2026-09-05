//! A terminal view of the population: one cell per node, in id order.
//!
//! The thing worth seeing is not who is alive but *how* what they hold got there, so the
//! default view colours each node by the way it acquired the gene that answers the
//! current stressor — inherited, conjugated, taken up from the dead, or delivered by a
//! phage. A shift arrives and the field goes dark; then it fills back in, mostly in the
//! colours of transfer, and that is the whole thesis in one screen.
//!
//! Keys: `q` / Esc / Ctrl-C quit, space pauses.

use crate::gene::{Acquisition, GeneId};
use crate::world::World;
use anyhow::{Context, Result};
use crossterm::style::{Color, Print, ResetColor, SetForegroundColor};
use crossterm::terminal::{Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::{cursor, event, execute, queue, terminal};
use std::io::{Stdout, Write, stdout};
use std::time::{Duration, Instant};

#[derive(clap::ValueEnum, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[clap(rename_all = "snake_case")]
pub enum ShowMode {
    /// Can this node answer the current stressor, and how did it get the gene that does?
    #[default]
    Resistance,
    /// Energy, as a shade.
    Energy,
    /// Strain — the thing the restriction barrier is measured against.
    Strain,
}

pub struct Renderer {
    out: Stdout,
    mode: ShowMode,
    min_frame: Duration,
    last_frame: Option<Instant>,
    paused: bool,
    buf: String,
}

impl Renderer {
    pub fn new(mode: ShowMode, fps: f64) -> Result<Renderer> {
        let mut out = stdout();
        terminal::enable_raw_mode().context("--render needs a terminal to draw on")?;
        execute!(out, EnterAlternateScreen, cursor::Hide, Clear(ClearType::All))?;
        let min_frame =
            if fps > 0.0 { Duration::from_secs_f64(1.0 / fps) } else { Duration::ZERO };
        Ok(Renderer { out, mode, min_frame, last_frame: None, paused: false, buf: String::new() })
    }

    /// Draw one frame. Returns `false` if the user asked to quit.
    pub fn frame(&mut self, world: &World, status: &str) -> Result<bool> {
        if let Some(last) = self.last_frame {
            let elapsed = last.elapsed();
            if elapsed < self.min_frame {
                std::thread::sleep(self.min_frame - elapsed);
            }
        }
        self.last_frame = Some(Instant::now());

        let (cols, rows) = terminal::size()?;
        let cols = cols.max(8) as usize;
        let rows = rows.max(3) as usize;
        let hazard = world.env.kind_at(world.tick);
        let resistance = world.env.resistance_gene(hazard);
        let wanted = crate::gene::fnv1a(&resistance);

        queue!(self.out, cursor::MoveTo(0, 0))?;
        let mut painted = 0usize;
        let capacity = cols * (rows - 1);
        let mut current: Option<Color> = None;
        self.buf.clear();
        for node in world.nodes.values().take(capacity) {
            let (glyph, color) = match self.mode {
                ShowMode::Resistance => resistance_cell(node, wanted),
                ShowMode::Energy => (energy_glyph(node.energy, world.cfg.energy_cap), Color::Grey),
                ShowMode::Strain => ('#', strain_color(node.strain)),
            };
            if current != Some(color) {
                if !self.buf.is_empty() {
                    queue!(self.out, Print(std::mem::take(&mut self.buf)))?;
                }
                queue!(self.out, SetForegroundColor(color))?;
                current = Some(color);
            }
            self.buf.push(glyph);
            painted += 1;
            if painted.is_multiple_of(cols) {
                queue!(self.out, Print(std::mem::take(&mut self.buf)), cursor::MoveToNextLine(1))?;
            }
        }
        if !self.buf.is_empty() {
            queue!(self.out, Print(std::mem::take(&mut self.buf)))?;
        }
        queue!(self.out, ResetColor, Clear(ClearType::FromCursorDown))?;
        queue!(
            self.out,
            cursor::MoveTo(0, rows as u16 - 1),
            Print(status),
            Clear(ClearType::UntilNewLine)
        )?;
        self.out.flush()?;
        self.input()
    }

    fn input(&mut self) -> Result<bool> {
        loop {
            let wait = if self.paused { Duration::from_millis(100) } else { Duration::ZERO };
            if !event::poll(wait)? {
                if self.paused {
                    continue;
                }
                return Ok(true);
            }
            if let event::Event::Key(key) = event::read()? {
                match key.code {
                    event::KeyCode::Char('q') | event::KeyCode::Esc => return Ok(false),
                    event::KeyCode::Char('c')
                        if key.modifiers.contains(event::KeyModifiers::CONTROL) =>
                    {
                        return Ok(false);
                    }
                    event::KeyCode::Char(' ') => self.paused = !self.paused,
                    _ => self.paused = false,
                }
            }
        }
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        let _ = execute!(self.out, ResetColor, cursor::Show, LeaveAlternateScreen);
        let _ = terminal::disable_raw_mode();
    }
}

/// A node's cell in the default view: whether it can answer the current stressor, and
/// how it came by the gene that does.
pub fn resistance_cell(node: &crate::node::Node, wanted: GeneId) -> (char, Color) {
    match node.genome.get(wanted) {
        None => ('.', Color::DarkGrey),
        Some(c) => (c.via.glyph(), acquisition_color(c.via)),
    }
}

pub fn acquisition_color(via: Acquisition) -> Color {
    match via {
        Acquisition::Founder => Color::White,
        Acquisition::Birth => Color::Green,
        Acquisition::Conjugation => Color::Cyan,
        Acquisition::Transformation => Color::Yellow,
        Acquisition::Transduction => Color::Magenta,
    }
}

pub fn energy_glyph(energy: u32, cap: u32) -> char {
    const SHADES: [char; 5] = ['.', ':', '+', '*', '#'];
    let cap = cap.max(1);
    let i = ((energy.min(cap) as u64 * (SHADES.len() as u64 - 1)) / cap as u64) as usize;
    SHADES[i]
}

pub fn strain_color(strain: u8) -> Color {
    const COLORS: [Color; 8] = [
        Color::Red,
        Color::Green,
        Color::Yellow,
        Color::Blue,
        Color::Magenta,
        Color::Cyan,
        Color::White,
        Color::DarkGrey,
    ];
    COLORS[(strain as usize) % COLORS.len()]
}

/// The status line: where the run is, and how much of what is held came sideways.
pub fn status(world: &World, lateral_share: f64) -> String {
    let hazard = world.env.kind_at(world.tick);
    format!(
        "tick {} epoch {} stressor {} | {} nodes | lateral {:.0}% | transfers {}/{} | q quit, space pause",
        world.tick,
        world.env.epoch_at(world.tick),
        hazard,
        world.population(),
        lateral_share * 100.0,
        world.stats.transfers,
        world.stats.attempts
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::HgtConfig;
    use crate::gene::fnv1a;

    #[test]
    fn a_node_is_painted_by_how_it_got_the_gene_that_matters_now() {
        let cfg = HgtConfig { nodes: 4, ..HgtConfig::default() };
        let world = World::new(cfg, 1).expect("valid config");
        let wanted = fnv1a(&world.env.resistance_gene(0));
        let node = world.nodes.values().next().expect("a founder");
        assert_eq!(resistance_cell(node, wanted), ('o', Color::White), "founders start with it");
        assert_eq!(resistance_cell(node, 12345), ('.', Color::DarkGrey), "and lack everything else");
        assert_eq!(acquisition_color(Acquisition::Conjugation), Color::Cyan);
    }

    #[test]
    fn energy_shades_span_the_range() {
        assert_eq!(energy_glyph(0, 400), '.');
        assert_eq!(energy_glyph(400, 400), '#');
        assert_eq!(energy_glyph(4000, 400), '#', "over the cap is still full");
        assert_eq!(energy_glyph(0, 0), '.', "a zero cap must not divide by zero");
    }
}
