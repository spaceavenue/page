use page_rs::ui::viewport::Viewport;

#[test]
fn dimensions_and_body_height() {
  let viewport = Viewport::new(80, 24);
  assert_eq!(viewport.width(), 80);
  assert_eq!(viewport.height(), 24);
  assert_eq!(viewport.body_height(), 23);
  assert_eq!(viewport.last_row(), 23);
}

#[test]
fn zero_height_has_no_body_rows() {
  let viewport = Viewport::new(80, 0);
  assert_eq!(viewport.body_height(), 0);
  assert_eq!(viewport.last_row(), 0);
}

#[test]
fn content_width_reserves_gutter_and_never_reaches_zero() {
  let viewport = Viewport::new(80, 24);
  assert_eq!(viewport.content_width(0), 80);
  assert_eq!(viewport.content_width(4), 76);
  assert_eq!(viewport.content_width(100), 1);
}

#[test]
fn bottom_line_saturates() {
  let viewport = Viewport::new(80, 24);
  assert_eq!(viewport.bottom_line(0), 23);
  assert_eq!(viewport.bottom_line(100), 123);
  assert_eq!(viewport.bottom_line(usize::MAX), usize::MAX);
}

#[test]
fn gutter_width_grows_with_line_count() {
  let viewport = Viewport::new(80, 24);
  assert_eq!(viewport.gutter_width(false, 1_000), 0);
  assert_eq!(viewport.gutter_width(true, 1), 4);
  assert_eq!(viewport.gutter_width(true, 120), 4);
  assert_eq!(viewport.gutter_width(true, 1_000), 5);
}

#[test]
fn gutter_disabled_on_narrow_terminals() {
  let viewport = Viewport::new(7, 24);
  assert_eq!(viewport.gutter_width(true, 1_000), 0);
}

#[test]
fn percent_zero_at_top_and_hundred_at_bottom() {
  let viewport = Viewport::new(80, 24);
  assert_eq!(viewport.percent(0, 120), 0);
  assert_eq!(viewport.percent(97, 120), 100);
  assert_eq!(viewport.percent(200, 120), 100);
}

#[test]
fn short_files_always_one_hundred_percent() {
  let viewport = Viewport::new(80, 24);
  assert_eq!(viewport.percent(0, 10), 100);
  assert_eq!(viewport.percent(0, 0), 100);
}
