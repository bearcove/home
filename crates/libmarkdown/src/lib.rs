use std::{collections::HashSet, sync::Arc};

use config::{TenantInfo, WebConfig};
use conflux::{Href, InputPath, InputPathRef, MarkdownRef, RevisionView, Toc};
use impls::{FormatterMode, options};
use pulldown_cmark::Parser;
use template::TemplateCollection;

mod impls;

impl Default for ModImpl {
    fn default() -> Self {
        ModImpl {
            highlight: highlight::load(),
            math: math::load(),
            media: media::load(),
        }
    }
}

struct ModImpl {
    highlight: &'static dyn highlight::Mod,
    math: &'static dyn math::Mod,
    media: &'static dyn media::Mod,
}

/// A markdown processor
#[dylo::export]
impl Mod for ModImpl {
    /// Collect all things that this markdown file depends on: images, shortcodes, etc.
    fn collect_dependencies(
        &self,
        args: ProcessMarkdownArgs<'_>,
    ) -> noteyre::Result<CollectDependenciesResult> {
        let parser = Parser::new_ext(args.markdown.as_str(), options());
        let mut formatter = self.mk_formatter(FormatterMode::JustCollectDependencies, args);
        formatter.drain_parser(parser)?;

        let res = CollectDependenciesResult {
            frontmatter: formatter.result.frontmatter,
            deps: formatter.result.deps,
        };
        Ok(res)
    }

    /// Actually renders the markdown to the writer
    fn process_markdown_to_writer(
        &self,
        args: ProcessMarkdownArgs<'_>,
    ) -> noteyre::Result<ProcessMarkdownResult> {
        let parser = Parser::new_ext(args.markdown.as_str(), options());
        let mut formatter = self.mk_formatter(FormatterMode::Render, args);
        formatter.drain_parser(parser)?;

        Ok(formatter.result)
    }

    fn basic_markdown(&self, input: &str) -> noteyre::Result<String> {
        use pulldown_cmark::{Parser, html};
        let parser = Parser::new(input);
        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);
        Ok(html_output)
    }
}

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
#[autotrait]
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
