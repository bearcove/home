use std::{collections::HashSet, sync::Arc};

use autotrait::autotrait;
use config_types::{TenantInfo, WebConfig};
use conflux::{Href, InputPath, InputPathRef, MarkdownRef, RevisionView, Toc};
use impls::{FormatterMode, options};
use markdown_types::{CollectDependenciesResult, ProcessMarkdownArgs, ProcessMarkdownResult};
use pulldown_cmark::Parser;
use template_types::TemplateCollection;

mod impls;

impl Default for ModImpl {
    fn default() -> Self {
        ModImpl {
            highlight: libhighlight::load(),
            math: libmath::load(),
            media: libmedia::load(),
        }
    }
}

struct ModImpl {
    highlight: &'static dyn libhighlight::Mod,
    math: &'static dyn libmath::Mod,
    media: &'static dyn libmedia::Mod,
}

/// A markdown processor
#[autotrait]
impl Mod for ModImpl {
    /// Collect all things that this markdown file depends on: images, shortcodes, etc.
    fn collect_dependencies(
        &self,
        args: ProcessMarkdownArgs<'_>,
    ) -> eyre::Result<CollectDependenciesResult> {
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
    ) -> eyre::Result<ProcessMarkdownResult> {
        let parser = Parser::new_ext(args.markdown.as_str(), options());
        let mut formatter = self.mk_formatter(FormatterMode::Render, args);
        formatter.drain_parser(parser)?;

        Ok(formatter.result)
    }

    fn basic_markdown(&self, input: &str) -> eyre::Result<String> {
        use pulldown_cmark::{Parser, html};
        let parser = Parser::new(input);
        let mut html_output = String::new();
        html::push_html(&mut html_output, parser);
        Ok(html_output)
    }
}
