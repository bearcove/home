pub struct HighlightCodeParams<'a> {
    /// the code to highlight
    pub source: &'a str,
    /// something like "rust" or "go" — whatever was
    /// in the fenced code block. it can be empty.
    pub tag: &'a str,
    /// written as `data-bo`
    pub byte_offset: usize,
}
