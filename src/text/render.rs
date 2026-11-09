use std::io::Write;
use std::ops::Range;

use crossterm::queue;
use crossterm::style::{
  Attribute, Print, SetAttribute, SetAttributes, SetBackgroundColor, SetForegroundColor,
};

use super::ParsedLine;
use super::ansi::Style;
use crate::text::Highlight;

impl ParsedLine {
  /// Render one display row, including the SGR commands needed to apply its
  /// style. The terminal style is reset at the end of the row.
  ///
  /// # Errors
  ///
  /// Returns an error if writing to `out` fails.
  pub fn render_range(
    &self,
    out: &mut impl Write,
    range: Range<usize>,
    highlights: &[Highlight],
  ) -> anyhow::Result<()> {
    if range.is_empty() {
      return Ok(());
    }

    let mut current = Style::default();

    let mut hi = highlights.partition_point(|h| h.range.end <= range.start);

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

      // let Some(piece) = self.text.get(start..end).filter(|piece| !piece.is_empty()) else {
      //   continue;
      // };

      let mut pos = start;
      while pos < end {
        while highlights.get(hi).is_some_and(|h| h.range.end <= pos) {
          hi += 1;
        }
        let (seg_end, style) = match highlights.get(hi) {
          Some(h) if h.range.start <= pos => (h.range.end.min(end), run.style.overlay(h.style)),
          Some(h) => (h.range.start.min(end), run.style),
          None => (end, run.style),
        };
        write_piece(out, self.text.get(pos..seg_end), style, &mut current)?;
        pos = seg_end;
      }
    }

    if current != Style::default() {
      queue!(out, SetAttribute(Attribute::Reset))?;
    }

    Ok(())
  }
}

fn write_piece(
  out: &mut impl Write,
  slice: Option<&str>,
  style: Style,
  current: &mut Style,
) -> anyhow::Result<()> {
  let Some(slice) = slice else {
    return Ok(());
  };
  if style != *current {
    apply_style(out, style)?;
    *current = style;
  }
  queue!(out, Print(slice))?;
  Ok(())
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
