use serde::Serialize;

/// An exact half-open byte range in the original source snapshot plus the
/// zero-based row/column coordinates of its two ends.
///
/// Invariants:
///
/// * `byte_start <= byte_end` and the range is half-open: `[byte_start, byte_end)`.
/// * rows are zero-based.
/// * columns are zero-based **UTF-8 byte** columns, never UTF-16 columns and
///   never display columns.
/// * the original source bytes are authoritative; newlines are never normalized
///   before positions are computed, so `LF` and `CRLF` sources report the
///   columns that actually exist in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
pub struct SourceRange {
    pub byte_start: u32,
    pub byte_end: u32,
    pub row_start: u32,
    pub column_start: u32,
    pub row_end: u32,
    pub column_end: u32,
}

impl SourceRange {
    pub fn new(
        byte_start: u32,
        byte_end: u32,
        row_start: u32,
        column_start: u32,
        row_end: u32,
        column_end: u32,
    ) -> Self {
        Self {
            byte_start,
            byte_end,
            row_start,
            column_start,
            row_end,
            column_end,
        }
    }

    /// Number of bytes covered by the range.
    pub fn byte_len(&self) -> u32 {
        self.byte_end - self.byte_start
    }

    pub fn is_empty(&self) -> bool {
        self.byte_start == self.byte_end
    }

    /// True when `self` fully encloses `other` (byte-wise, inclusive of equality).
    pub fn contains(&self, other: &SourceRange) -> bool {
        self.byte_start <= other.byte_start && other.byte_end <= self.byte_end
    }

    /// True when the two ranges share at least one byte.
    pub fn overlaps(&self, other: &SourceRange) -> bool {
        self.byte_start < other.byte_end && other.byte_start < self.byte_end
    }

    /// Compact, stable rendering used by canonical fact output and tests.
    pub fn render(&self) -> String {
        format!(
            "{}..{} ({}:{}..{}:{})",
            self.byte_start,
            self.byte_end,
            self.row_start,
            self.column_start,
            self.row_end,
            self.column_end
        )
    }
}

/// Compute the zero-based `(row, column)` for a byte offset.
///
/// `column` counts UTF-8 bytes from the start of the line, matching
/// Tree-sitter's `Point::column` for UTF-8 input.
///
/// This builds a [`LineIndex`] for the whole source, so it is `O(source.len())`
/// and allocates. Use [`LineIndex`] directly when more than a handful of offsets
/// are needed.
pub fn point_at(source: &[u8], byte_offset: u32) -> (u32, u32) {
    LineIndex::new(source).point_at(byte_offset)
}

/// Precomputed line-start table for `O(log n)` row/column lookup.
///
/// Node ranges come from Tree-sitter, which already knows its own points. This
/// index exists for the ranges RepoDex derives itself, such as a declaration
/// header that ends where a body begins. Scanning the source for every such
/// range is quadratic in the file size, so the index is built once per file.
#[derive(Debug, Clone)]
pub struct LineIndex {
    /// Byte offset of the first byte of each line. Always starts with `0`.
    starts: Vec<u32>,
    /// Length of the indexed source in bytes.
    len: u32,
}

impl LineIndex {
    pub fn new(source: &[u8]) -> Self {
        let mut starts = Vec::with_capacity(source.len() / 32 + 1);
        starts.push(0u32);
        for (index, byte) in source.iter().enumerate() {
            if *byte == b'\n' {
                starts.push(index as u32 + 1);
            }
        }
        Self {
            starts,
            len: source.len() as u32,
        }
    }

    /// Zero-based `(row, column)` with `column` counted in UTF-8 bytes.
    ///
    /// Offsets past the end of the source are clamped to the end, so a
    /// malformed offset can never panic or produce an out-of-range column.
    pub fn point_at(&self, byte_offset: u32) -> (u32, u32) {
        let offset = byte_offset.min(self.len);
        let row = self.starts.partition_point(|start| *start <= offset) - 1;
        (row as u32, offset - self.starts[row])
    }

    /// Build a [`SourceRange`] from raw byte offsets.
    pub fn range(&self, byte_start: u32, byte_end: u32) -> SourceRange {
        let (row_start, column_start) = self.point_at(byte_start);
        let (row_end, column_end) = self.point_at(byte_end);
        SourceRange::new(
            byte_start,
            byte_end,
            row_start,
            column_start,
            row_end,
            column_end,
        )
    }
}

/// Build a [`SourceRange`] from raw byte offsets by scanning the source.
///
/// This builds a [`LineIndex`] for every call, so it is `O(source.len())`. It
/// is intended for one-off use outside the extraction loop.
pub fn range_from_offsets(source: &[u8], byte_start: u32, byte_end: u32) -> SourceRange {
    LineIndex::new(source).range(byte_start, byte_end)
}
