//! A bordered, width-bounded "card" for verify output.
//!
//! Word-wrapping is delegated to [`textwrap`] — no mid-word cuts, clean
//! continuation lines that start at a column, never a fragment. The frame
//! is a fixed-width box. Every line's width is measured on the **plain**
//! (un-styled) text, never the ANSI-styled form, so the right edge stays
//! aligned whether or not `anstream` strips the color downstream (TTY vs.
//! piped / CI).
//!
//! Long *commands* are deliberately kept OUT of the card by callers and
//! printed below it, so they stay on one copy-pasteable line.

use anstyle::Style;
use textwrap::core::display_width;
use textwrap::{Options, WordSplitter};

/// Base wrap options: never split a word at a hyphen — identifiers and
/// paths (`wal-header`, `file-existence`, `.db-wal`) must survive whole.
fn base_opts() -> Options<'static> {
    Options::new(INNER).word_splitter(WordSplitter::NoHyphenation)
}

/// Total card width including borders. Content wraps to `WIDTH - 4`
/// (`│ ` + content + ` │`). Chosen so the whole card fits in 88 columns.
pub(crate) const WIDTH: usize = 88;
const INNER: usize = WIDTH - 4;

/// A card under construction. Push rows, then [`render`](Card::render).
pub(crate) struct Card {
    rows: Vec<Row>,
}

struct Row {
    /// The styled (possibly ANSI-bearing) content.
    styled: String,
    /// Its visible width, measured on the plain text.
    width: usize,
}

impl Card {
    pub(crate) fn new() -> Self {
        Self { rows: Vec::new() }
    }

    pub(crate) fn blank(&mut self) {
        self.rows.push(Row {
            styled: String::new(),
            width: 0,
        });
    }

    /// A single, uniformly-styled line indented `indent` spaces. Not
    /// wrapped — for short structured rows (labels, diff rows, ids).
    pub(crate) fn line(&mut self, text: &str, style: Style, indent: usize) {
        let pad = " ".repeat(indent);
        self.rows.push(Row {
            styled: format!("{pad}{style}{text}{style:#}"),
            width: indent + display_width(text),
        });
    }

    /// A pre-styled line that mixes several styles; `plain` is its visible
    /// form, used only to measure width.
    pub(crate) fn raw(&mut self, styled: String, plain: &str) {
        self.rows.push(Row {
            styled,
            width: display_width(plain),
        });
    }

    /// A word-wrapped paragraph, uniformly styled, indented `indent` spaces.
    pub(crate) fn wrap(&mut self, text: &str, style: Style, indent: usize) {
        let ind = " ".repeat(indent);
        let opts = base_opts().initial_indent(&ind).subsequent_indent(&ind);
        self.push_wrapped(text, style, &opts);
    }

    /// A word-wrapped paragraph with a hanging `label`: the first line
    /// starts with the label, continuations align under the text. Uniformly
    /// styled (label included).
    pub(crate) fn wrap_hang(&mut self, label: &str, text: &str, style: Style) {
        let subs = " ".repeat(display_width(label));
        let opts = base_opts().initial_indent(label).subsequent_indent(&subs);
        self.push_wrapped(text, style, &opts);
    }

    fn push_wrapped(&mut self, text: &str, style: Style, opts: &textwrap::Options<'_>) {
        for line in textwrap::wrap(text, opts) {
            let width = display_width(&line);
            self.rows.push(Row {
                styled: format!("{style}{line}{style:#}"),
                width,
            });
        }
    }

    /// Render the framed card (with a trailing newline). The border is plain
    /// (no ANSI, so the box never needs ANSI-aware width math); content keeps
    /// whatever styling the caller applied. A row wider than the inner width
    /// is not padded — its right border shifts out rather than the text being
    /// cut (callers keep long commands out of the card for this reason).
    pub(crate) fn render(&self) -> String {
        let rule = "─".repeat(INNER + 2);
        let mut out = format!("┌{rule}┐\n");
        for row in &self.rows {
            let pad = " ".repeat(INNER.saturating_sub(row.width));
            out.push_str(&format!("│ {}{pad} │\n", row.styled));
        }
        out.push_str(&format!("└{rule}┘\n"));
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_rendered_line_fits_the_width_when_plain() {
        // A long paragraph wraps; no plain line exceeds WIDTH, and words
        // are never split mid-token.
        let mut c = Card::new();
        c.line("✗ PROPERTY VIOLATED  some_canon_id", Style::new(), 0);
        c.blank();
        c.wrap(
            "The WAL initialized flag is set true only after a successful sync of the wal-header, \
             which means a non-durable header that merely exists on disk must not read as present.",
            Style::new(),
            0,
        );
        c.wrap_hang(
            "  why  ",
            "turso reports initialized from the file-existence proxy, so a header write that \
             landed before the sync failed still reads as present even though it is not durable.",
            Style::new(),
        );
        let rendered = c.render();
        // Every rendered line — rules and content alike — is exactly WIDTH
        // columns, so the borders align in a monospace terminal.
        for line in rendered.lines() {
            assert_eq!(
                display_width(line),
                WIDTH,
                "every card line must be exactly {WIDTH} cols; got {:?}",
                line
            );
        }
        // No word was split: "wal-header" and "file-existence" survive whole.
        assert!(rendered.contains("wal-header"));
        assert!(rendered.contains("file-existence"));
    }

    #[test]
    fn borders_align_for_styled_and_plain_rows() {
        // A styled row and a plain row of the same visible text pad to the
        // same right-border column (width is measured on the plain text).
        let bold = Style::new().bold();
        let mut a = Card::new();
        a.line("initialized = false", bold, 2);
        let mut b = Card::new();
        b.line("initialized = false", Style::new(), 2);
        let a_line = a.render().lines().nth(1).unwrap().to_string();
        let b_line = b.render().lines().nth(1).unwrap().to_string();
        assert_eq!(display_width(&a_line), display_width(&b_line));
        assert_eq!(display_width(&b_line), WIDTH);
    }
}
