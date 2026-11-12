//! A LIFO string scratch buffer for caching intermediate encoder output.
//!
//! Encoders write pieces that repeat across many lines, such as a metric name
//! or a rendered label set, into a [`ScratchBuf`] once. They then refer to
//! those pieces by [`Span`] and copy them into the output as often as needed.
//! The buffer is used as a stack: record an [`Offset`] before pushing and
//! [`ScratchBuf::rewind`] to it once the pushed pieces are no longer needed.

use crate::encoding::text::TextBuf;
use std::fmt;

/// A position in a [`ScratchBuf`], as returned by [`ScratchBuf::offset`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Offset(usize);

/// A byte range of a [`ScratchBuf`], as returned by [`ScratchBuf::span_since`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Span {
    start: usize,
    end: usize,
}

/// A string buffer used in a LIFO manner.
#[derive(Debug, Default)]
pub(crate) struct ScratchBuf {
    buf: String,
    /// Staging buffer for [`ScratchBuf::encode_text`]. It is always empty
    /// between calls and only kept around to reuse its allocation.
    staging: String,
}

impl ScratchBuf {
    /// Creates an empty buffer.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns the offset of the current top of the buffer.
    #[inline]
    pub(crate) fn offset(&self) -> Offset {
        Offset(self.buf.len())
    }

    /// Pushes `s` onto the top of the buffer.
    #[inline]
    pub(crate) fn push_str(&mut self, s: &str) {
        self.buf.push_str(s);
    }

    /// Returns the span covering everything pushed since `offset`.
    #[inline]
    pub(crate) fn span_since(&self, offset: Offset) -> Span {
        let end = self.buf.len();
        debug_assert!(
            offset.0 <= end,
            "ScratchBuf::span_since from stale offset {} (len {end})",
            offset.0,
        );
        Span {
            start: offset.0,
            end,
        }
    }

    /// Borrows the text covered by `span`.
    ///
    /// Returns `None` if the span is out of bounds or does not lie on `char`
    /// boundaries, e.g. because the buffer was rewound below its end.
    #[inline]
    pub(crate) fn get(&self, span: Span) -> Option<&str> {
        self.buf.get(span.start..span.end)
    }

    /// Truncates the buffer back to `offset`, discarding everything pushed
    /// after it.
    ///
    /// Rewinding to an offset past the current top is a LIFO violation. It
    /// panics in debug builds and is a no-op in release builds.
    #[inline]
    pub(crate) fn rewind(&mut self, offset: Offset) {
        debug_assert!(
            offset.0 <= self.buf.len(),
            "ScratchBuf::rewind to stale offset {} (len {})",
            offset.0,
            self.buf.len()
        );
        self.buf.truncate(offset.0);
    }

    /// Runs `f` with a [`TextBuf`] whose output is appended to the top of the
    /// buffer, so text encoders such as `LabelList` can write into it.
    pub(crate) fn encode_text(
        &mut self,
        f: impl FnOnce(TextBuf<'_>) -> fmt::Result,
    ) -> fmt::Result {
        debug_assert!(self.staging.is_empty());
        let result = f(TextBuf::new(&mut self.staging, &mut self.buf));
        self.buf.push_str(&self.staging);
        self.staging.clear();
        result
    }
}

impl fmt::Write for ScratchBuf {
    #[inline]
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.push_str(s);
        Ok(())
    }

    #[inline]
    fn write_char(&mut self, c: char) -> fmt::Result {
        self.buf.push(c);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::encoding::text::{LabelList, FLUSH_THRESHOLD};
    use std::fmt::Write;

    #[test]
    fn push_and_get() {
        let mut buf = ScratchBuf::new();
        buf.push_str("foo");
        let start = buf.offset();
        buf.push_str("bar");
        let span = buf.span_since(start);
        assert_eq!(buf.get(span), Some("bar"));
        assert_eq!(buf.get(buf.span_since(buf.offset())), Some(""));
    }

    #[test]
    fn rewind_restores_prior_content() {
        let mut buf = ScratchBuf::new();
        buf.push_str("a");
        let outer = buf.offset();
        buf.push_str("b");
        let outer_span = buf.span_since(outer);
        let inner = buf.offset();
        buf.push_str("c");
        // Spans pushed earlier stay valid while later pieces are on top.
        assert_eq!(buf.get(outer_span), Some("b"));
        assert_eq!(buf.get(buf.span_since(inner)), Some("c"));

        buf.rewind(inner);
        assert_eq!(buf.get(outer_span), Some("b"));
        buf.rewind(outer);
        assert_eq!(buf.get(buf.span_since(Offset(0))), Some("a"));
    }

    #[test]
    fn get_after_rewind_is_none() {
        let mut buf = ScratchBuf::new();
        let start = buf.offset();
        buf.push_str("foo");
        let span = buf.span_since(start);
        buf.rewind(start);
        assert_eq!(buf.get(span), None);
    }

    #[test]
    fn get_off_char_boundary_is_none() {
        let mut buf = ScratchBuf::new();
        buf.push_str("é");
        let span = Span { start: 1, end: 2 };
        assert_eq!(buf.get(span), None);
    }

    #[test]
    fn write_through_fmt_write() {
        let mut buf = ScratchBuf::new();
        write!(buf, "{}-", 1).unwrap();
        buf.write_char('x').unwrap();
        assert_eq!(buf.get(buf.span_since(Offset(0))), Some("1-x"));
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "stale offset")]
    fn rewind_to_stale_offset_panics() {
        let mut buf = ScratchBuf::new();
        buf.push_str("foo");
        let end = buf.offset();
        buf.rewind(Offset(0));
        buf.rewind(end);
    }

    #[test]
    #[cfg(debug_assertions)]
    #[should_panic(expected = "stale offset")]
    fn span_since_stale_offset_panics() {
        let mut buf = ScratchBuf::new();
        buf.push_str("foo");
        let end = buf.offset();
        buf.rewind(Offset(0));
        let _ = buf.span_since(end);
    }

    #[test]
    fn encode_labels_through_encode_text() {
        let mut buf = ScratchBuf::new();
        buf.push_str("prefix{");
        let start = buf.offset();
        let mut list = LabelList::new("");
        buf.encode_text(|text| list.encode(text, &[("method", "GET"), ("status", "200")]))
            .unwrap();
        let labels = buf.span_since(start);
        assert_eq!(buf.get(labels), Some(r#"method="GET",status="200""#));
        buf.rewind(start);
        assert_eq!(buf.get(buf.span_since(Offset(0))), Some("prefix{"));
    }

    #[test]
    fn encode_text_keeps_output_past_flush_threshold() {
        let mut buf = ScratchBuf::new();
        buf.push_str("keep");
        let start = buf.offset();
        let value = "v".repeat(2 * FLUSH_THRESHOLD);
        let mut list = LabelList::new("");
        buf.encode_text(|text| list.encode(text, &[("key", value.as_str())]))
            .unwrap();
        let expected = format!("key=\"{value}\"");
        assert_eq!(buf.get(buf.span_since(start)), Some(expected.as_str()));
        assert_eq!(
            buf.get(buf.span_since(Offset(0))).map(|s| &s[..4]),
            Some("keep")
        );
    }

    #[test]
    fn encode_text_appends_staged_output_on_error() {
        let mut buf = ScratchBuf::new();
        let result = buf.encode_text(|mut text| {
            text.write_str("partial")?;
            Err(fmt::Error)
        });
        assert!(result.is_err());
        assert_eq!(buf.get(buf.span_since(Offset(0))), Some("partial"));
        assert!(buf.staging.is_empty());

        // The staging buffer is reusable after an error.
        buf.encode_text(|mut text| text.write_str("!")).unwrap();
        assert_eq!(buf.get(buf.span_since(Offset(0))), Some("partial!"));
    }
}
