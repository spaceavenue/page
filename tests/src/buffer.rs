use page_rs::buffer::Buffer;

fn buffer_from(contents: &[u8]) -> Buffer {
  Buffer::from_bytes(contents.to_vec(), "test")
}

#[test]
fn indexing_through_line_makes_it_readable() {
  let mut buf = buffer_from(b"hello\nworld\n");
  buf.index_till_line(1);
  assert_eq!(buf.line_bytes(0), Some(&b"hello"[..]));
  assert_eq!(buf.line_bytes(1), Some(&b"world"[..]));
  assert_eq!(buf.line_bytes(2), None);
  assert_eq!(buf.line_count(), 2);
}

#[test]
fn unterminated_last_line_readable() {
  let mut buf = buffer_from(b"a\nb\nc");
  buf.index_till_line(2);
  assert_eq!(buf.line_bytes(0), Some(&b"a"[..]));
  assert_eq!(buf.line_bytes(1), Some(&b"b"[..]));
  assert_eq!(buf.line_bytes(2), Some(&b"c"[..]));
  assert_eq!(buf.line_count(), 3);
}
