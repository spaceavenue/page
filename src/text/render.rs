use std::io::Write;
use std::ops::Range;

use crossterm::queue;
use crossterm::style::{
  Attribute, Print, SetAttribute, SetAttributes, SetBackgroundColor, SetForegroundColor,
};

use super::ParsedLine;
use super::ansi::Style;

impl ParsedLine {
  /// Render one display row, including the SGR commands needed to apply its
  /// style. The terminal style is reset at the end of the row.
  ///
  /// # Errors
  ///
  /// Returns an error if writing to `out` fails.
  pub fn render_range(&self, out: &mut impl Write, range: Range<usize>) -> anyhow::Result<()> {
    if range.is_empty() {
      return Ok(());
    }

    let mut current = Style::default();

    let first = self
      .runs
      .partition_point(|run| run.start <= range.start)
      .saturating_sub(1);
    let runs = self.runs.get(first..).unwrap_or_default();

    for (offset, run) in runs.iter().enumerate() {
      if run.start >= range.end {
        break;
      }

      let start = run.start.max(range.start);
      let next_start = runs
        .get(offset + 1)
        .map_or(self.text.len(), |next| next.start);
      let end = next_start.min(range.end);

      let Some(piece) = self.text.get(start..end).filter(|piece| !piece.is_empty()) else {
        continue;
      };

      if run.style != current {
        apply_style(out, run.style)?;
        current = run.style;
      }
      queue!(out, Print(piece))?;
    }

    if current != Style::default() {
      queue!(out, SetAttribute(Attribute::Reset))?;
    }

    Ok(())
  }
}

fn apply_style(out: &mut impl Write, style: Style) -> anyhow::Result<()> {
  queue!(out, SetAttribute(Attribute::Reset))?;

  if let Some(foreground) = style.fg {
    queue!(out, SetForegroundColor(foreground))?;
  }
  if let Some(background) = style.bg {
    queue!(out, SetBackgroundColor(background))?;
  }

  if !style.attrs.is_empty() {
    queue!(out, SetAttributes(style.attrs))?;
  }

  Ok(())
}
