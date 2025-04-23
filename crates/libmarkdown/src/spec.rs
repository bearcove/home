/// A markdown processor
pub trait Mod: Sync + Send + 'static {
    /// Collect all things that this markdown file depends on: images, shortcodes, etc.
    fn collect_dependencies(
        &self,
        args: ProcessMarkdownArgs<'_>,
    ) -> eyre::Result<CollectDependenciesResult>;

    /// Actually renders the markdown to the writer
    fn process_markdown_to_writer(
        &self,
        args: ProcessMarkdownArgs<'_>,
    ) -> eyre::Result<ProcessMarkdownResult>;
}
