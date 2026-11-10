use unicode_width::UnicodeWidthStr;

use super::state::SearchCursor;
use crate::pager::Pager;
use crate::scroll::Anchor;
use crate::search::SearchHit;
use crate::text::WrapMode;

/// Rows of context kept above a match when a jump has to scroll.
const SCROLL_MARGIN_ROWS: usize = 2;
/// Columns of context kept left of a match when a jump has to scroll horizontally.
const SCROLL_MARGIN_COLS: usize = 8;

impl Pager {
  /// Sets hit in pager, scrolls if not already on screen, then remember it as the search cursor.
  pub(super) fn jump_to_match(&mut self, hit: SearchHit) {
    self.following = false;
    if let Some(raw) = self.buffer.line_bytes(hit.line) {
      self
        .parsed_buf
        .parse_line(raw, self.opts.ansi, self.opts.tab_width, hit.line);
      match self.opts.wrap {
        WrapMode::None => self.reveal_nowrap(&hit),
        WrapMode::Soft | WrapMode::Hard => self.reveal_wrapped(&hit),
      }
    }
    self.search_state.cursor = Some(SearchCursor { hit, top: self.top });
    self.dirty();
  }

  /// In `text::WrapMode::Soft` and `text::WrapMode::Hard` scroll vertically so every row of match
  /// is on screen.
  fn reveal_wrapped(&mut self, hit: &SearchHit) {
    let body = self.viewport.body_height();
    let width = self.content_width();
    let last_byte = hit.range.end.saturating_sub(1).max(hit.range.start);

    // soft wrapping can drop whitespace at a break. compare against each row's `end`.
    let mut start_row = None;
    let mut end_row = 0;
    for (index, range) in self.parsed_buf.rows(width, self.opts.wrap).enumerate() {
      end_row = index;
      if start_row.is_none() && hit.range.start < range.end {
        start_row = Some(index);
      }
      if last_byte < range.end {
        break;
      }
    }
    let start_row = start_row.unwrap_or(end_row);
    let span = end_row.saturating_sub(start_row) + 1;

    let first = Anchor {
      line: hit.line,
      row: start_row,
    };
    // match taller than the screen can never be fully visible. settle for its first row.
    let fits = span <= body;
    let last = if fits {
      Anchor {
        line: hit.line,
        row: end_row,
      }
    } else {
      first
    };

    // `advance` needs the rows to be indexed or it counts them as one row.
    self
      .buffer
      .index_till_line(self.viewport.bottom_line(self.top.line));
    let bottom = self.advance(self.top, body.saturating_sub(1));
    if self.top <= first && last <= bottom {
      return;
    }

    let margin = if fits {
      SCROLL_MARGIN_ROWS.min(body.saturating_sub(span) / 2)
    } else {
      0
    };
    let rewinded = self.rewind(first, margin);
    self.set_top(rewinded);
  }

  /// In `text::WrapMode::None` scroll vertically to the hit line and horizontally to the match
  /// columns.
  fn reveal_nowrap(&mut self, hit: &SearchHit) {
    let body = self.viewport.body_height();
    let width = self.content_width();
    let visible_lines = self.top.line..self.viewport.bottom_line(self.top.line);
    if !visible_lines.contains(&hit.line) {
      let margin = SCROLL_MARGIN_ROWS.min(body.saturating_sub(1) / 2);
      self.set_top(Anchor {
        line: hit.line.saturating_sub(margin),
        row: 0,
      });
    }

    let text = self.parsed_buf.text();
    let start_col = UnicodeWidthStr::width(text.get(..hit.range.start).unwrap_or_default());
    let end_col = UnicodeWidthStr::width(text.get(..hit.range.end).unwrap_or_default());
    let span = end_col.saturating_sub(start_col);

    // match wider than the window can't be fully shown. settle for its first column.
    let fits = span <= width;
    let needed_end = if fits { end_col } else { start_col + 1 };
    if start_col >= self.left && needed_end <= self.left + width {
      return;
    }

    let margin = if fits {
      SCROLL_MARGIN_COLS.min(width.saturating_sub(span) / 2)
    } else {
      0
    };
    self.left = start_col.saturating_sub(margin);
  }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
  use crate::pager::search::navigate::SearchOutcome::{Found, Wrapped};
  use crate::pager::test_util::{at, numbered_doc, set_query, test_pager};
  use crate::scroll::Anchor;
  use crate::text::WrapMode;

  #[test]
  fn visible_match_does_not_scroll() {
    let doc = numbered_doc(30, &[1, 3], " target");
    let mut pager = test_pager(&doc, 80, 6); // 5 body rows
    set_query(&mut pager, "target");

    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().line, 1);
    assert_eq!(pager.top, Anchor::default());

    // the cursor must still advance even though the screen did not move
    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().line, 3);
    assert_eq!(pager.top, Anchor::default());
  }

  #[test]
  fn hidden_match_scrolls_margin() {
    let doc = numbered_doc(100, &[50], " target");
    let mut pager = test_pager(&doc, 80, 11); // 10 body rows
    set_query(&mut pager, "target");

    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().line, 50);
    assert_eq!(pager.top, at(48, 0));
  }

  #[test]
  fn wrapping_scrolls_and_moves_cursor() {
    let doc = numbered_doc(100, &[1, 50], " target");
    let mut pager = test_pager(&doc, 80, 11); // 10 body rows
    set_query(&mut pager, "target");

    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().line, 50);
    assert_eq!(pager.top, at(48, 0));

    // line 1 is off screen: the wrap has to scroll back up to it
    assert_eq!(pager.search_next(), Wrapped);
    assert_eq!(pager.last_match().unwrap().line, 1);
    assert_eq!(pager.top, Anchor::default());

    // and the cursor moved with it, so the next search continues forward instead of wrapping again
    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.last_match().unwrap().line, 50);
  }

  #[test]
  fn wrapped_match_fully_visible() {
    // The match spans bytes 15..21: the end of row 1 and the start of row 2 at width 10.
    let mut doc = numbered_doc(20, &[], "");
    doc.push_str("aaaaaaaaaabbbbbneedleccccc\n");
    // lines after the match, so the end of the document does not clamp the scroll position
    doc.push_str(&numbered_doc(10, &[], ""));
    let mut pager = test_pager(&doc, 10, 6); // 5 body rows
    pager.opts.wrap = WrapMode::Hard;
    set_query(&mut pager, "needle");

    assert_eq!(pager.search_next(), Found);
    let hit = pager.last_match().unwrap().clone();
    assert_eq!((hit.line, hit.range), (20, 15..21));

    // one row of margin above the first row of the match
    assert_eq!(pager.top, at(20, 0));
    let bottom = pager.advance(pager.top, 4);
    assert!(bottom >= at(20, 2));
  }

  #[test]
  fn match_taller_than_screen_shows_first_row() {
    let mut doc = numbered_doc(20, &[], "");
    doc.push_str("aaaaaaaaaabbbbbneedleccccc\n");
    let mut pager = test_pager(&doc, 10, 2); // 1 body row
    pager.opts.wrap = WrapMode::Hard;
    set_query(&mut pager, "needle");

    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.top, at(20, 1));
  }

  #[test]
  fn nowrap_scrolls_horizontally_to_match() {
    let doc = format!("{}needle{}\n", "x".repeat(50), "y".repeat(50));
    let mut pager = test_pager(&doc, 20, 3);
    pager.opts.wrap = WrapMode::None;
    set_query(&mut pager, "needle");

    assert_eq!(pager.search_next(), Found);
    // columns 50..56 with a 7 column margin: the window is 43..63
    assert_eq!(pager.left, 43);

    // already fully visible: stays put
    pager.search_state.cursor = None;
    assert_eq!(pager.search_next(), Found);
    assert_eq!(pager.left, 43);
  }
}
