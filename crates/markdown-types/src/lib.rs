use std::{collections::HashSet, sync::Arc};

use config_types::{TenantInfo, WebConfig};
use conflux::{Href, InputPath, InputPathRef, MarkdownRef, RevisionView, Toc};
use template_types::TemplateCollection;

pub struct CollectDependenciesResult {
    // the YAML frontmatter, to be parsed later
    pub frontmatter: Option<String>,

    // dependencies
    pub deps: HashSet<InputPath>,
}

pub struct ProcessMarkdownArgs<'a> {
    pub path: &'a InputPathRef,
    pub markdown: &'a MarkdownRef,
    pub w: &'a mut dyn std::io::Write,
    pub rv: Arc<dyn RevisionView>,
    pub ti: Arc<TenantInfo>,
    pub templates: &'a dyn TemplateCollection,
    pub web: WebConfig,
}

/// The result of processing markdown
pub struct ProcessMarkdownResult {
    // the YAML frontmatter, to be parsed later
    pub frontmatter: Option<String>,

    // dependencies
    pub deps: HashSet<InputPath>,

    // the table of contents (to be stored as JSON and later formatted by the template)
    pub toc: Toc,

    // no formatting at all, just the text (useful for excerpts)
    pub plain_text: String,

    // estimated reading time
    pub reading_time: i64,

    // all links
    pub links: HashSet<Href>,
}
