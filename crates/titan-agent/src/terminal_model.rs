use crate::theme;
use alacritty_terminal::{
    event::VoidListener,
    index::{Column, Line},
    term::{
        Config, Term, TermMode,
        cell::{Cell, Flags},
        test::TermSize,
    },
    vte::ansi::{Color, NamedColor, Processor, Rgb},
};
use titan_agent_core::keys::KeyModes;
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Span {
    pub text: String,
    pub fg: u32,
    pub bg: u32,
    pub bold: bool,
    pub italic: bool,
    pub underline: bool,
    pub strikeout: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StyledLine {
    pub spans: Vec<Span>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellSpan {
    pub column: u16,
    pub cells: u16,
    pub span: Span,
    pub clusters: Vec<CellCluster>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CellCluster {
    pub column: u16,
    pub cells: u16,
    pub text: String,
}
pub struct TerminalModel {
    term: Term<VoidListener>,
    parser: Processor,
    size: (u16, u16),
}
impl TerminalModel {
    pub fn new(cols: u16, rows: u16) -> Self {
        let size = TermSize::new(usize::from(cols.max(2)), usize::from(rows.max(1)));
        Self {
            term: Term::new(Config::default(), &size, VoidListener),
            parser: Processor::new(),
            size: (cols, rows),
        }
    }
    pub fn feed(&mut self, bytes: &[u8]) {
        for byte in bytes {
            self.parser
                .advance(&mut self.term, std::slice::from_ref(byte))
        }
    }
    pub fn resize(&mut self, cols: u16, rows: u16) {
        self.size = (cols, rows);
        self.term.resize(TermSize::new(
            self.size.0.max(2) as usize,
            self.size.1.max(1) as usize,
        ));
    }
    pub fn size(&self) -> (u16, u16) {
        self.size
    }
    pub fn cursor(&self) -> Option<(u16, u16)> {
        if self.size.0 == 0 || self.size.1 == 0 || !self.term.mode().contains(TermMode::SHOW_CURSOR)
        {
            return None;
        }
        let p = self.term.grid().cursor.point;
        let row = p.line.0.clamp(0, i32::from(self.size.1.saturating_sub(1))) as u16;
        let col = p.column.0.min(usize::from(self.size.0.saturating_sub(1))) as u16;
        Some((row, col))
    }
    pub fn key_modes(&self) -> KeyModes {
        KeyModes {
            app_cursor: self.term.mode().contains(TermMode::APP_CURSOR),
            bracketed_paste: self.term.mode().contains(TermMode::BRACKETED_PASTE),
        }
    }
    pub fn lines(&self) -> Vec<StyledLine> {
        self.cell_lines()
            .into_iter()
            .map(|line| StyledLine {
                spans: line.into_iter().map(|item| item.span).collect(),
            })
            .collect()
    }
    pub fn cell_lines(&self) -> Vec<Vec<CellSpan>> {
        let mut result = Vec::new();
        for row in 0..self.size.1 {
            let mut spans: Vec<CellSpan> = Vec::new();
            let mut col = 0;
            while col < usize::from(self.size.0) {
                let cell = &self.term.grid()[Line(row as i32)][Column(col)];
                let flags = cell.flags;
                let (mut fg, mut bg) = colors(cell);
                let bold = flags.contains(Flags::BOLD);
                if bold && let Color::Named(n) = cell.fg {
                    let index = n as usize;
                    if index < 8 {
                        fg = theme::ANSI[index + 8];
                    }
                }
                if bold && let Color::Indexed(index @ 0..=7) = cell.fg {
                    fg = theme::ANSI[index as usize + 8];
                }
                if flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg)
                }
                if flags.contains(Flags::DIM) {
                    fg = mix(fg, bg)
                }
                if flags.contains(Flags::HIDDEN) {
                    fg = bg
                }
                let style = (
                    fg,
                    bg,
                    bold,
                    flags.contains(Flags::ITALIC),
                    flags.intersects(Flags::ALL_UNDERLINES),
                    flags.contains(Flags::STRIKEOUT),
                );
                if flags.contains(Flags::WIDE_CHAR_SPACER) {
                    col += 1;
                    continue;
                }
                let remaining = usize::from(self.size.0).saturating_sub(col);
                let wide = flags.contains(Flags::WIDE_CHAR);
                let mut text = String::new();
                text.push(if wide && remaining < 2 { ' ' } else { cell.c });
                if !(wide && remaining < 2)
                    && let Some(z) = cell.zerowidth()
                {
                    text.extend(z)
                }
                let width = if wide && remaining >= 2 { 2 } else { 1 };
                if let Some(last) = spans.last_mut().filter(|s| {
                    (
                        s.span.fg,
                        s.span.bg,
                        s.span.bold,
                        s.span.italic,
                        s.span.underline,
                        s.span.strikeout,
                    ) == style
                }) {
                    last.span.text.push_str(&text);
                    last.cells = last.cells.saturating_add(width as u16);
                    last.clusters.push(CellCluster {
                        column: col as u16,
                        cells: width as u16,
                        text,
                    });
                } else {
                    spans.push(CellSpan {
                        column: col as u16,
                        cells: width as u16,
                        clusters: vec![CellCluster {
                            column: col as u16,
                            cells: width as u16,
                            text: text.clone(),
                        }],
                        span: Span {
                            text,
                            fg,
                            bg,
                            bold,
                            italic: style.3,
                            underline: style.4,
                            strikeout: style.5,
                        },
                    })
                }
                col += width;
            }
            result.push(spans);
        }
        result
    }
}
fn colors(cell: &Cell) -> (u32, u32) {
    (color(cell.fg), color(cell.bg))
}
fn color(c: Color) -> u32 {
    match c {
        Color::Indexed(i) => theme::indexed_color(i),
        Color::Spec(Rgb { r, g, b }) => (u32::from(r) << 16) | (u32::from(g) << 8) | u32::from(b),
        Color::Named(n) => match n {
            NamedColor::Foreground => theme::TERMINAL_FG,
            NamedColor::Background => theme::TERMINAL_BG,
            NamedColor::Cursor => theme::TERMINAL_CURSOR,
            NamedColor::DimBlack => theme::ANSI[0],
            NamedColor::DimRed => theme::ANSI[1],
            NamedColor::DimGreen => theme::ANSI[2],
            NamedColor::DimYellow => theme::ANSI[3],
            NamedColor::DimBlue => theme::ANSI[4],
            NamedColor::DimMagenta => theme::ANSI[5],
            NamedColor::DimCyan => theme::ANSI[6],
            NamedColor::DimWhite => theme::ANSI[7],
            _ => theme::ANSI[(n as usize).min(15)],
        },
    }
}
fn mix(fg: u32, bg: u32) -> u32 {
    let ch = |shift: u32| ((2 * ((fg >> shift) & 255) + ((bg >> shift) & 255)) / 3) & 255;
    (ch(16) << 16) | (ch(8) << 8) | ch(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_one_column_size_clamps_cursor_and_output_width() {
        let mut model = TerminalModel::new(1, 2);
        model.feed("界".as_bytes());
        assert_eq!(model.size(), (1, 2));
        assert_eq!(model.cursor(), Some((0, 0)));
        assert_eq!(model.lines().len(), 2);
        assert_eq!(
            model.lines()[0]
                .spans
                .iter()
                .map(|s| s.text.as_str())
                .collect::<String>()
                .chars()
                .count(),
            1
        );
    }

    #[test]
    fn zero_declared_dimensions_are_preserved() {
        let mut model = TerminalModel::new(0, 0);
        assert_eq!(model.size(), (0, 0));
        assert!(model.lines().is_empty());
        assert_eq!(model.cursor(), None);
        model.resize(0, 2);
        assert_eq!(model.size(), (0, 2));
        assert_eq!(model.lines().len(), 2);
        assert!(model.lines().iter().all(|line| line.spans.is_empty()));
    }

    #[test]
    fn bold_brightens_low_indexed_foreground_colors() {
        for index in 0..=7 {
            let mut model = TerminalModel::new(4, 1);
            model.feed(format!("\x1b[1;38;5;{index}mX").as_bytes());
            assert_eq!(
                model.lines()[0].spans[0].fg,
                theme::ANSI[index as usize + 8]
            );
        }
    }

    #[test]
    fn cell_spans_report_their_start_columns_and_widths() {
        let mut model = TerminalModel::new(8, 1);
        model.feed("A\x1b[31m界\x1b[32mB".as_bytes());
        let spans = model.cell_lines();
        assert_eq!(
            spans[0]
                .iter()
                .map(|s| (s.column, s.cells))
                .collect::<Vec<_>>(),
            vec![(0, 1), (1, 2), (3, 1), (4, 4)]
        );
    }
}
