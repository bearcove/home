use std::ops::Range;
use std::sync::LazyLock;
use std::{
    collections::{HashMap, VecDeque},
    fmt,
};

use bo_inserter::BoInserter;
use conflux::{BsForResults, Href, Media};
use media::MediaMarkupOpts;
use noteyre::{bail, eyre};
use pulldown_cmark::{
    Alignment, CodeBlockKind, CowStr, Event, HeadingLevel, LinkType, MetadataBlockKind, Options,
    Parser, Tag, TagEnd,
};
use saphyr::{Yaml, yaml::YamlLoader};

use conflux::{InputPath, InputPathRef, Markdown, TocEntry};
use math::MathMode;
use slug::slugify;
use template_types::{DataObject, DataValue};

mod argparse;
use argparse::parse_emphasis_shortcode;

use crate::{ModImpl, ProcessMarkdownArgs, ProcessMarkdownResult};

mod bo_inserter;

// So block shortcodes have:
//  - Blockquote
//    - Paragraph
//      - Emphasis
//
// We need dummy stack item types to pop off when
// the emphasis, paragraph, and blockquote end, so
// we don't accidentally write the corresponding closing
// HTML tag.

// Canonicalize some links: for example, `https://crates.io/crates/FOOBAR`
// will be rewritten to `https://lib.rs/crates/FOOBAR`, because it has a better
// interface.
fn canonicalize_link_url(input: &str) -> String {
    // Implementing link transformation
    let mut output = input.to_string();

    // Use a regex or simple string manipulation to replace 'https://crates.io/crates/' with 'https://lib.rs/crates/'
    if let Some(pos) = output.find("https://crates.io/crates/") {
        output.replace_range(
            pos.."https://crates.io/crates/".len() + pos,
            "https://lib.rs/crates/",
        );
    }

    output
}

static MARKDOWN_TRACING: LazyLock<bool> =
    LazyLock::new(|| std::env::var("MARKDOWN_TRACING") == Ok("1".to_string()));

macro_rules! trace {
    ($($msg:tt)*) => {
        if *MARKDOWN_TRACING {
            eprintln!($($msg)*);
        }
    };
}

#[derive(Debug)]
pub(crate) enum StackItem<'a> {
    ShortPlus {
        plain_text: String,
    },
    ShortBlockStart {
        plain_text: String,
    },
    ShortBlockEnd {
        after_body: Markdown,
    },
    Paragraph,
    Link,
    Emphasis,
    Strong,
    Strikethrough,
    OrderedList,
    UnorderedList,
    Item,
    Blockquote,
    HtmlBlock,
    Image(ImageItem<'a>),
    Frontmatter {
        plain_text: String,
    },
    CodeBlock {
        lang: CowStr<'a>,
        plain_text: String,
        byte_offset: usize,
    },
    Heading {
        plain_text: String,
        html: Vec<u8>,
    },
    Table {
        alignments: Vec<Alignment>,
    },
    TableRow {
        // how many cells we've already written (so we can know the alignment)
        column_index: usize,
    },
}

#[derive(Clone, Copy, Debug)]
enum PopPushKind {
    /// in a regular context, open/close `<em>` tags etc.
    Regular,

    /// in a plaintext context, like collecting plaintext for an image's alt text,
    /// don't write tags at all
    Plaintext,
}

impl PopPushKind {
    fn is_regular(&self) -> bool {
        matches!(self, Self::Regular)
    }
}

impl<'a> StackItem<'a> {
    fn substack(&mut self) -> Option<&mut Vec<StackItem<'a>>> {
        match self {
            StackItem::Image(image) => Some(&mut image.substack),
            _ => None,
        }
    }
}

#[derive(Debug)]
pub(crate) struct ImageItem<'a> {
    pub(crate) link_type: LinkType,
    pub(crate) dest_url: CowStr<'a>,
    pub(crate) title: CowStr<'a>,
    pub(crate) id: CowStr<'a>,

    pub(crate) plain_text: String,

    pub(crate) substack: Vec<StackItem<'a>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatterMode {
    Render,
    JustCollectDependencies,
}

pub(crate) struct Formatter<'a> {
    pub(crate) stack: Vec<StackItem<'a>>,

    pub(crate) footnote_counter: usize,
    pub(crate) footnotes: HashMap<String, usize>,

    pub(crate) args: ProcessMarkdownArgs<'a>,
    pub(crate) result: ProcessMarkdownResult,

    pub(crate) highlight: &'static dyn highlight::Mod,
    pub(crate) math: &'static dyn math::Mod,
    pub(crate) media: &'static dyn media::Mod,

    pub(crate) mode: FormatterMode,

    pub(crate) discard_writer: DiscardWriter,

    /// used in reading time estimates
    pub(crate) num_prose_bytes: usize,
    pub(crate) num_code_lines: usize,
}

impl fmt::Debug for Formatter<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("State").field("stack", &self.stack).finish()
    }
}

impl<'a> Formatter<'a> {
    fn get_footnote_index(&mut self, label: &str) -> usize {
        if let Some(index) = self.footnotes.get(label) {
            *index
        } else {
            let index = self.footnote_counter;
            self.footnote_counter += 1;
            self.footnotes.insert(label.to_string(), index);
            index
        }
    }

    fn writer(&mut self) -> noteyre::Result<&mut dyn std::io::Write> {
        if self.mode == FormatterMode::JustCollectDependencies {
            return Ok(&mut self.discard_writer);
        }

        for item in self.stack.iter_mut().rev() {
            match item {
                StackItem::Heading { html, .. } => return Ok(html as &mut dyn std::io::Write),
                StackItem::CodeBlock { .. } => bail!("fenced code blocks don't have markup"),
                _ => continue,
            }
        }
        Ok(self.args.w)
    }

    fn push(&mut self, item: StackItem<'a>) -> noteyre::Result<PopPushKind> {
        if let Some(last_mut) = self.last_mut() {
            if let Some(substack) = last_mut.substack() {
                substack.push(item);
                return Ok(PopPushKind::Plaintext);
            }
        }

        self.stack.push(item);
        Ok(PopPushKind::Regular)
    }

    fn pop(&mut self) -> Option<(StackItem<'a>, PopPushKind)> {
        if let Some(last_mut) = self.last_mut() {
            if let Some(substack) = last_mut.substack() {
                if let Some(item) = substack.pop() {
                    return Some((item, PopPushKind::Plaintext));
                }
            }
        }

        let res = self.stack.pop().map(|item| (item, PopPushKind::Regular));

        trace!("Popped {res:?}");
        res
    }

    fn last_mut(&mut self) -> Option<&mut StackItem<'a>> {
        self.stack.last_mut()
    }

    fn write_plain_text(&mut self, text: &str) {
        let mut counts_as_plaintext = true;
        let mut is_code_block = false;

        for item in self.stack.iter_mut().rev() {
            match item {
                StackItem::Heading { plain_text, .. } => {
                    plain_text.push_str(text);
                    break;
                }
                StackItem::CodeBlock { plain_text, .. } => {
                    is_code_block = true;
                    plain_text.push_str(text);
                    break;
                }
                StackItem::Frontmatter { plain_text, .. } => {
                    plain_text.push_str(text);
                    counts_as_plaintext = false;
                    break;
                }
                StackItem::ShortPlus { plain_text, .. } => {
                    plain_text.push_str(text);
                    counts_as_plaintext = false;
                    break;
                }
                StackItem::ShortBlockStart { plain_text } => {
                    plain_text.push_str(text);
                    counts_as_plaintext = false;
                    break;
                }
                StackItem::Image(image) => {
                    image.plain_text.push_str(text);
                    break;
                }
                _ => continue,
            }
        }

        if counts_as_plaintext {
            self.result.plain_text.push_str(text);
            if is_code_block {
                if text.contains('\n') {
                    self.num_code_lines += 1;
                }
            } else {
                self.num_prose_bytes += text.len();
            }
        }
    }

    fn write_raw_html(&mut self, text: &str) -> noteyre::Result<()> {
        if self.mode == FormatterMode::JustCollectDependencies {
            return Ok(());
        }

        match self.last_mut() {
            Some(StackItem::Frontmatter { .. }) => {
                // ignore
            }
            Some(StackItem::ShortPlus { .. }) => {
                // ignore
            }
            Some(StackItem::Image { .. }) => {
                // ignore, only grab plain text
            }
            _ => {
                // good
                self.writer()?.write_all(text.as_bytes())?;
            }
        }
        Ok(())
    }

    fn escape_and_write_html(&mut self, text: &str) -> noteyre::Result<()> {
        if self.mode == FormatterMode::JustCollectDependencies {
            return Ok(());
        }

        match self.last_mut() {
            Some(StackItem::Frontmatter { .. }) => {
                // ignore
            }
            Some(StackItem::ShortPlus { .. }) => {
                // ignore
            }
            Some(StackItem::CodeBlock { .. }) => {
                // ignore
            }
            Some(StackItem::Image { .. }) => {
                // ignore, only grab plain text
            }
            Some(StackItem::ShortBlockStart { .. }) => {
                // ignore, only grab plain text
            }
            _ => {
                html_escape::encode_safe_to_writer(text, &mut self.writer()?)?;
            }
        }

        Ok(())
    }

    fn start_code_block(&mut self, lang: CowStr<'a>, byte_offset: usize) -> noteyre::Result<()> {
        self.push(StackItem::CodeBlock {
            lang,
            plain_text: Default::default(),
            byte_offset,
        })?;
        Ok(())
    }
}

pub(crate) fn options() -> Options {
    Options::ENABLE_TABLES
        | Options::ENABLE_SMART_PUNCTUATION
        | Options::ENABLE_FOOTNOTES
        | Options::ENABLE_STRIKETHROUGH
        | Options::ENABLE_TASKLISTS
        | Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_YAML_STYLE_METADATA_BLOCKS
        | Options::ENABLE_PLUSES_DELIMITED_METADATA_BLOCKS
        | Options::ENABLE_MATH
        | Options::ENABLE_GFM
}

type EvPair<'a> = (Event<'a>, Range<usize>);

impl<'a> Formatter<'a> {
    fn process_event(&mut self, ev_buf: &mut VecDeque<EvPair<'a>>) -> noteyre::Result<()> {
        let ev = match ev_buf.pop_front() {
            Some(ev) => ev,
            None => return Ok(()),
        };

        trace!("{}, {:#?}", self.args.path, DebugWrapper(&ev.0));
        macro_rules! assert_pop {
            ($state:expr, $expected:pat => $fields:expr) => {
                match $state.pop() {
                    Some($expected) => $fields,
                    _ => {
                        bail!("Wanted to pop but {self:?}");
                    }
                }
            };
        }

        macro_rules! assert_peek {
            ($state:expr, $expected:pat => $fields:expr) => {
                match $state.last_mut() {
                    Some($expected) => $fields,
                    _ => {
                        bail!("Wanted to peek but {self:?}");
                    }
                }
            };
        }

        let (ev, range) = ev;

        match &ev {
            Event::Start(tag) => match tag {
                Tag::MetadataBlock(MetadataBlockKind::YamlStyle) => {
                    self.push(StackItem::Frontmatter {
                        plain_text: Default::default(),
                    })?;
                }
                Tag::MetadataBlock(MetadataBlockKind::PlusesStyle) => {
                    self.push(StackItem::ShortPlus {
                        plain_text: Default::default(),
                    })?;
                }
                #[allow(unused_variables)]
                Tag::Heading {
                    level,
                    id,
                    classes,
                    attrs,
                } => {
                    self.push(StackItem::Heading {
                        plain_text: Default::default(),
                        html: Default::default(),
                    })?;
                }
                #[allow(unused_variables)]
                Tag::Link {
                    link_type,
                    dest_url,
                    title,
                    id,
                } => {
                    let dest_url = canonicalize_link_url(dest_url);

                    self.result.links.insert(Href::new(dest_url.to_string()));

                    let w = self.writer()?;
                    write!(w, "<a href=\"{dest_url}\"")?;
                    if !title.is_empty() {
                        write!(w, " title=\"{title}\"")?;
                    }
                    if !id.is_empty() {
                        write!(w, " id=\"{id}\"")?;
                    }
                    write!(w, ">")?;
                    self.push(StackItem::Link)?;
                }
                Tag::Emphasis => match self.last_mut() {
                    Some(StackItem::Image(image)) => {
                        image.substack.push(StackItem::Emphasis);
                    }
                    _ => {
                        self.writer()?.write_all(b"<em>")?;
                        self.push(StackItem::Emphasis)?;
                    }
                },
                Tag::Strong => {
                    self.writer()?.write_all(b"<strong>")?;
                    self.push(StackItem::Strong)?;
                }
                Tag::Strikethrough => {
                    self.writer()?.write_all(b"<del>")?;
                    self.push(StackItem::Strikethrough)?;
                }
                Tag::Paragraph => {
                    trace!("Writing paragraph start");
                    write!(self.writer()?, r#"<p data-bo="{}">"#, range.start)?;
                    self.push(StackItem::Paragraph)?;
                }
                Tag::List(first_item_num) => {
                    if let Some(num) = first_item_num {
                        write!(self.writer()?, "<ol start=\"{num}\">")?;
                        self.push(StackItem::OrderedList)?;
                    } else {
                        self.writer()?.write_all(b"<ul>")?;
                        self.push(StackItem::UnorderedList)?;
                    }
                    self.result.plain_text.push('\n');
                }
                Tag::Item => match self.last_mut() {
                    Some(StackItem::UnorderedList) | Some(StackItem::OrderedList) => {
                        self.writer()?.write_all(b"<li>")?;
                        self.push(StackItem::Item)?;
                    }
                    _ => {
                        bail!("Unexpected: wanted to end list item, self={self:?}");
                    }
                },
                Tag::BlockQuote(_kind) => {
                    // lookahead: if we have `paragraph` and `emphasis`, then it's a "ShortBlock".
                    // in this case, we eat both the `paragraph` and `emphasis`: we'll push
                    // the `paragraph` again later.

                    let mut is_shortblock = false;

                    let mut lookahead = ev_buf.iter();
                    if let (
                        Some((Event::Start(Tag::Paragraph), _)),
                        Some((Event::Start(Tag::Emphasis), _)),
                        Some((Event::Text(text), _)),
                    ) = (lookahead.next(), lookahead.next(), lookahead.next())
                    {
                        if text.starts_with(':') {
                            trace!("shortblock starting with {text}!");
                            is_shortblock = true;
                        }
                    }

                    if is_shortblock {
                        // pop off the `paragraph` and `emphasis` tags
                        ev_buf.pop_front();
                        ev_buf.pop_front();

                        self.push(StackItem::ShortBlockStart {
                            plain_text: Default::default(),
                        })?;
                    } else {
                        trace!("blockquote starting!");
                        self.writer()?.write_all(b"<blockquote>")?;
                        self.push(StackItem::Blockquote)?;
                    }
                }
                Tag::CodeBlock(CodeBlockKind::Indented) => {
                    self.start_code_block("text".into(), range.start)?;
                }
                Tag::CodeBlock(CodeBlockKind::Fenced(lang)) => {
                    self.start_code_block(lang.clone(), range.start)?;
                }
                Tag::HtmlBlock => {
                    self.push(StackItem::HtmlBlock)?;
                }
                Tag::Image {
                    link_type,
                    dest_url,
                    title,
                    id,
                } => {
                    if !matches!(link_type, LinkType::Inline) {
                        bail!("unexpected link type: {:?}", link_type);
                    }
                    self.push(StackItem::Image(ImageItem {
                        link_type: *link_type,
                        dest_url: dest_url.clone(),
                        title: title.clone(),
                        id: id.clone(),
                        plain_text: Default::default(),
                        substack: Default::default(),
                    }))?;
                }
                Tag::Table(alignments) => {
                    self.push(StackItem::Table {
                        alignments: alignments.clone(),
                    })?;
                    self.writer()?.write_all(
                        format!(
                            "<div class=\"responsive-table\" data-bo=\"{}\"><table>",
                            range.start
                        )
                        .as_bytes(),
                    )?;
                }
                Tag::TableHead => {
                    self.push(StackItem::TableRow { column_index: 0 })?;
                    self.writer()?.write_all(b"<thead>")?;
                }
                Tag::TableRow => {
                    self.push(StackItem::TableRow { column_index: 0 })?;
                    self.writer()?.write_all(b"<tr>")?;
                }
                Tag::TableCell => {
                    let our_index = {
                        let column_index_slot = assert_peek!(self, StackItem::TableRow { column_index, .. } => column_index);
                        let our_index = *column_index_slot;
                        *column_index_slot += 1;
                        our_index
                    };
                    let alignment = self
                        .stack
                        .get(self.stack.len() - 2)
                        .and_then(|item| match item {
                            StackItem::Table { alignments } => alignments.get(our_index).copied(),
                            _ => None,
                        })
                        .unwrap_or(Alignment::None);

                    self.writer()?.write_all(b"<td")?;
                    let align_class = match alignment {
                        Alignment::Left => Some("align-l"),
                        Alignment::Center => Some("align-c"),
                        Alignment::Right => Some("align-r"),
                        Alignment::None => None,
                    };
                    match align_class {
                        Some(class) => write!(self.writer()?, " class=\"{class}\">{}", class)?,
                        None => write!(self.writer()?, ">")?,
                    }
                }
                Tag::FootnoteDefinition(label) => {
                    let index = self.get_footnote_index(label);
                    let w = self.writer()?;
                    write!(w, "<div id=\"fn:{label}\" class=\"footnote-definition\">")?;
                    write!(w, "<sup>{index}</sup> ")?;
                    write!(
                        w,
                        "<a href=\"#fnref:{label}\" class=\"footnote-backref\">&#8617;</a>"
                    )?;
                }
            },
            Event::End(tag) => match tag {
                TagEnd::MetadataBlock(MetadataBlockKind::YamlStyle) => match self.pop() {
                    Some((StackItem::Frontmatter { plain_text }, _)) => {
                        self.result.frontmatter.replace(plain_text);
                    }
                    _ => {
                        bail!("Unexpected: wanted to end frontmatter, self={self:?}");
                    }
                },
                TagEnd::MetadataBlock(MetadataBlockKind::PlusesStyle) => match self.pop() {
                    Some((StackItem::ShortPlus { plain_text }, _)) => {
                        // parse shortcode arguments as YAML
                        trace!("Parsing shortcode arguments as YAML: {plain_text:?}");
                        let yaml_values = YamlLoader::load_from_str(&plain_text).bs()?;
                        if yaml_values.len() != 1 {
                            bail!("Expected 1 YAML value, got {yaml_values:?}");
                        }
                        let yaml_value = yaml_values.into_iter().next().unwrap();
                        let yaml_value = match yaml_value {
                            Yaml::Hash(h) => h,
                            _ => {
                                bail!("Expected YAML Hash, got {yaml_value:?}");
                            }
                        };
                        if yaml_value.len() != 1 {
                            bail!(
                                "Expected only one key in the YAML Hash, got multiple keys: {yaml_value:?}"
                            );
                        }
                        let (shortcode_name, shortcode_args) = yaml_value.iter().next().unwrap();
                        let shortcode_name = match shortcode_name {
                            Yaml::String(s) if s.starts_with(':') => &s[1..],
                            _ => bail!("Shortcode name must be a string starting with ':'"),
                        };

                        {
                            let mut shortcode_args = yaml_to_data_object(shortcode_args)?;
                            self.insert_shortcode_globals(&mut shortcode_args);
                            let mut buffer = Vec::new();

                            let render_res = self
                                .args
                                .templates
                                .render_shortcode_to(
                                    &mut BoInserter::new(&mut buffer, range.start),
                                    template_types::Shortcode {
                                        name: shortcode_name,
                                        args: shortcode_args,
                                        body: None,
                                    },
                                    self.args.rv.clone(),
                                    self.args.web,
                                )
                                .bs()?;

                            // eprintln!(
                            //     "Assets looked up in shortcode {:?}: {:?}",
                            //     render_res.shortcode_input_path, render_res.assets_looked_up
                            // );
                            self.result.deps.insert(render_res.shortcode_input_path);
                            self.result.deps.extend(render_res.assets_looked_up);

                            let buffer_str = Markdown::new(String::from_utf8(buffer)?);
                            self.process_nested_markdown(&buffer_str)?
                        }
                    }
                    _ => {
                        bail!("Unexpected: wanted to end frontmatter, self={self:?}");
                    }
                },
                TagEnd::Heading(level) => match self.pop() {
                    Some((StackItem::Heading { plain_text, html }, _)) => {
                        let level = match level {
                            HeadingLevel::H1 => 1,
                            HeadingLevel::H2 => 2,
                            HeadingLevel::H3 => 3,
                            HeadingLevel::H4 => 4,
                            HeadingLevel::H5 => 5,
                            HeadingLevel::H6 => 6,
                        };

                        let slug = slugify(&plain_text);

                        self.write_plain_text("\n");

                        let w = self.writer()?;
                        let hash = '#';
                        write!(
                            w,
                            r#"<a id="{slug}" href="{hash}{slug}" class="anchor"><h{level}>"#
                        )?;
                        w.write_all(&html)?;
                        writeln!(w, "</h{level}></a>")?;

                        if !self.stack.iter().any(|item| {
                            matches!(
                                item,
                                StackItem::Blockquote | StackItem::ShortBlockEnd { .. }
                            )
                        }) {
                            self.result.toc.push(TocEntry {
                                level,
                                text: plain_text.trim().to_owned(),
                                slug,
                            });
                        }
                    }
                    _ => {
                        bail!("Unexpected: wanted to end heading, self={self:?}");
                    }
                },
                TagEnd::Link => match self.pop() {
                    Some((StackItem::Link, pp)) => {
                        if pp.is_regular() {
                            self.writer()?.write_all(b"</a>")?;
                        }
                    }
                    other => {
                        bail!("Unexpected: wanted to end link, got {other:?}");
                    }
                },
                TagEnd::Emphasis => {
                    let (item, pp) = self.pop().unwrap();
                    match item {
                        StackItem::Emphasis => {
                            if pp.is_regular() {
                                self.writer()?.write_all(b"</em>")?;
                            }
                        }
                        StackItem::ShortBlockStart { plain_text } => {
                            trace!("Time to render the shortblock: {plain_text:?}");

                            // now we can parse the shortcode args, first strip the colon
                            let plain_text = plain_text.trim_start_matches(':');

                            let (shortcode_name, mut shortcode_args) =
                                parse_emphasis_shortcode(plain_text)?;
                            self.insert_shortcode_globals(&mut shortcode_args);

                            let after_body;
                            {
                                // and evaluate the shortcode. pass `___BODY_MARKER___` as the body
                                // so we can split the "before body" and "after body" parts, write the
                                // "before body" part now, and write the "after body" part later
                                let mut buffer = Vec::new();
                                let render_res = self
                                    .args
                                    .templates
                                    .render_shortcode_to(
                                        &mut BoInserter::new(&mut buffer, range.start),
                                        template_types::Shortcode {
                                            name: &shortcode_name,
                                            args: shortcode_args,
                                            body: Some("___BODY_MARKER___"),
                                        },
                                        self.args.rv.clone(),
                                        self.args.web,
                                    )
                                    .bs()?;
                                // eprintln!(
                                //     "Assets looked up in shortcode {:?}: {:?}",
                                //     render_res.shortcode_input_path, render_res.assets_looked_up
                                // );
                                self.result.deps.insert(render_res.shortcode_input_path);
                                self.result.deps.extend(render_res.assets_looked_up);

                                let buffer_str = String::from_utf8(buffer)?;
                                let mut parts = buffer_str.splitn(2, "___BODY_MARKER___");
                                // make them both owned
                                let before_body = Markdown::new(
                                    parts
                                        .next()
                                        .ok_or(eyre!("Missing before body part"))?
                                        .to_string(),
                                );
                                after_body = Markdown::new(
                                    parts
                                        .next()
                                        .ok_or(eyre!("Missing after body part"))?
                                        .to_string(),
                                );

                                trace!(
                                    "before_body=\n\n{before_body}\n\nafter_body={after_body}\n\n"
                                );

                                // write `before_body` now
                                self.process_nested_markdown(&before_body)?;

                                trace!("done writing before_body");
                            };

                            // now we need to push a shortblockend and a paragraph (since we ate those)
                            self.push(StackItem::ShortBlockEnd { after_body })?;

                            if let Some((Event::End(TagEnd::Paragraph), _)) = ev_buf.front() {
                                trace!("avoiding writing empty paragraph");
                                ev_buf.pop_front();
                            } else {
                                self.writer()?.write_all(b"<p>")?;
                                self.push(StackItem::Paragraph)?;
                            }

                            trace!("pushed shortblockend and paragraph");
                        }
                        other => {
                            bail!(
                                "At emphasis end, expected emphasis or shortblock, got {other:#?}"
                            );
                        }
                    }
                }
                TagEnd::Strong => {
                    let (item, pp) = self.pop().unwrap();
                    match item {
                        StackItem::Strong => {
                            if pp.is_regular() {
                                self.writer()?.write_all(b"</strong>")?;
                            }
                        }
                        other => {
                            bail!("Unexpected: wanted to end strong, got {other:?}");
                        }
                    }
                }
                TagEnd::Strikethrough => {
                    let (item, pp) = self.pop().unwrap();
                    match item {
                        StackItem::Strikethrough => {
                            if pp.is_regular() {
                                self.writer()?.write_all(b"</del>")?;
                            }
                        }
                        other => {
                            bail!("Unexpected: wanted to end strikethrough, got {other:?}");
                        }
                    }
                }
                TagEnd::Paragraph => {
                    let (item, pp) = self.pop().unwrap();
                    match item {
                        StackItem::Paragraph => {
                            if pp.is_regular() {
                                self.writer()?.write_all(b"</p>\n\n")?;
                                self.result.plain_text.push('\n');
                            }
                        }
                        other => {
                            bail!("Unexpected: wanted to end paragraph, got {other:?}");
                        }
                    }
                }
                TagEnd::List(_ordered) => match self.pop() {
                    Some((StackItem::OrderedList, _)) => {
                        self.writer()?.write_all(b"</ol>\n")?;
                    }
                    Some((StackItem::UnorderedList, _)) => {
                        self.writer()?.write_all(b"</ul>\n")?;
                    }
                    _ => {
                        bail!("Unexpected: wanted to end list, self={self:?}");
                    }
                },
                TagEnd::Item => match self.pop() {
                    Some((StackItem::Item, pp)) => {
                        if pp.is_regular() {
                            self.writer()?.write_all(b"</li>\n")?;
                            self.result.plain_text.push('\n');
                        }
                    }
                    other => {
                        bail!("Unexpected: wanted to end item, got {other:?}");
                    }
                },
                TagEnd::BlockQuote => {
                    match self.pop() {
                        Some((item, pp)) => match item {
                            StackItem::Blockquote => {
                                if pp.is_regular() {
                                    self.writer()?.write_all(b"</blockquote>\n")?;
                                }
                            }
                            StackItem::ShortBlockEnd { after_body } => {
                                // now we can write the rest of the body
                                self.process_nested_markdown(&after_body)?;
                            }
                            other => {
                                bail!(
                                    "At blockquote end, expected blockquote or shortblockend, got {other:#?}"
                                );
                            }
                        },
                        None => {
                            bail!("Unexpected: wanted to end blockquote, but stack is empty");
                        }
                    }
                }
                TagEnd::CodeBlock => {
                    let highlight = self.highlight;
                    let (lang, plain_text, byte_offset) = assert_pop!(self, (StackItem::CodeBlock { lang, plain_text, byte_offset }, _) => (lang, plain_text, byte_offset));
                    let w = self.writer()?;
                    highlight
                        .highlight_code(
                            w,
                            highlight::HighlightCodeParams {
                                source: &plain_text,
                                tag: &lang,
                                byte_offset,
                            },
                        )
                        .bs()?;
                }
                TagEnd::HtmlBlock => {
                    assert_pop!(self, (StackItem::HtmlBlock, _) => ());
                }
                TagEnd::Image => {
                    let image = assert_pop!(self, (StackItem::Image(image), _) => image);
                    self.process_image(&image)?;
                }
                TagEnd::Table => match self.pop() {
                    Some((StackItem::Table { .. }, pp)) => {
                        if pp.is_regular() {
                            self.writer()?.write_all(b"</table></div>")?;
                        }
                    }
                    other => {
                        bail!("Unexpected: wanted to end table, got {other:?}");
                    }
                },
                TagEnd::TableRow => match self.pop() {
                    Some((StackItem::TableRow { .. }, pp)) => {
                        if pp.is_regular() {
                            self.writer()?.write_all(b"</tr>")?;
                        }
                    }
                    other => {
                        bail!("Unexpected: wanted to end table row, got {other:?}");
                    }
                },
                TagEnd::TableHead => match self.pop() {
                    Some((StackItem::TableRow { .. }, pp)) => {
                        if pp.is_regular() {
                            self.writer()?.write_all(b"</thead>")?;
                        }
                    }
                    other => {
                        bail!("Unexpected: wanted to end table head, got {other:?}");
                    }
                },
                TagEnd::TableCell => {
                    self.writer()?.write_all(b"</td>")?;
                }
                TagEnd::FootnoteDefinition => {
                    self.writer()?.write_all(b"</div>")?;
                }
            },
            Event::FootnoteReference(label) => {
                let index = self.get_footnote_index(label);
                let w = self.writer()?;
                write!(
                    w,
                    "<sup id=\"fnref:{label}\" class=\"footnote-ref\"><a href=\"#fn:{label}\">[{index}]</a></sup>"
                )?;
            }
            Event::InlineHtml(html_in) => {
                self.write_raw_html(html_in)?;
            }
            Event::SoftBreak => {
                self.write_plain_text(" ");
                self.writer()?.write_all(b"\n")?;
            }
            Event::HardBreak => {
                self.writer()?.write_all(b"<br>")?;
            }
            Event::Rule => {
                self.writer()?.write_all(b"<hr>")?;
            }
            Event::Text(text) => {
                self.write_plain_text(text);
                self.escape_and_write_html(text)?;
            }
            Event::Html(html_in) => {
                self.write_raw_html(html_in)?;
            }
            Event::InlineMath(math) => {
                self.math
                    .render_math(math, MathMode::Inline, self.writer()?)
                    .bs()?;
            }
            Event::DisplayMath(math) => {
                self.math
                    .render_math(math, MathMode::Block, self.writer()?)
                    .bs()?;
            }
            Event::Code(code) => {
                self.writer()?.write_all(b"<code>")?;
                self.escape_and_write_html(code)?;
                self.writer()?.write_all(b"</code>")?;

                self.write_plain_text(code)
            }
            _ => {
                eprintln!("unsupported event: {ev:?}, self={self:?}");
            }
        }

        Ok(())
    }

    fn process_image(&mut self, image_item: &ImageItem<'a>) -> noteyre::Result<()> {
        let ImageItem {
            link_type,
            dest_url,
            title,
            id,
            plain_text,
            substack,
        } = image_item;

        if !substack.is_empty() {
            bail!("unexpected image with non-empty substack: {:?}", substack);
        }

        if !matches!(link_type, LinkType::Inline) {
            bail!("unexpected link type: {:?}", link_type);
        }
        if dest_url.starts_with("http:") || dest_url.starts_with("https:") {
            bail!("refusing to hotlink media: {dest_url}");
        }

        let path = if dest_url.starts_with('/') {
            InputPath::new(dest_url.trim_end_matches('/').to_string())
        } else {
            self.args
                .path
                .canonicalize_relative_path(InputPathRef::from_str(dest_url))
        };
        self.result.deps.insert(path.clone());

        if self.mode != FormatterMode::Render {
            return Ok(());
        }

        let media: Media = {
            let buster = self.args.rv.cachebuster();
            buster.media(&path)?.clone()
        };
        let alt = if plain_text.is_empty() {
            None
        } else {
            Some(plain_text.as_str())
        };
        let mm_opts = MediaMarkupOpts {
            path: &path,
            media: &media,
            rv: self.args.rv.as_ref(),
            id: if id.is_empty() { None } else { Some(id) },
            title: if title.is_empty() { alt } else { Some(title) },
            alt,
            width: None,  // keep original width
            height: None, // keep original height
            class: None,
            web: self.args.web,
        };
        let markup = self.media.media_html_markup(mm_opts)?;
        self.writer()?.write_all(markup.as_bytes())?;

        Ok(())
    }

    pub(crate) fn drain_parser(&mut self, parser: Parser<'a>) -> noteyre::Result<()> {
        let mut ev_buf = VecDeque::new();
        let mut eof = false;
        static BUF_SIZE: usize = 4;

        let mut offset_iter = parser.into_offset_iter();

        loop {
            // try to buffer up to 3 events
            while ev_buf.len() < BUF_SIZE {
                if let Some((ev, range)) = offset_iter.next() {
                    ev_buf.push_back((ev, range));
                } else {
                    eof = true;
                    break;
                }
            }
            self.process_event(&mut ev_buf)?;

            if eof && ev_buf.is_empty() {
                break;
            }
        }

        self.estimate_reading_time();
        Ok(())
    }

    fn estimate_reading_time(&mut self) {
        // Assuming an average of 5 bytes per word
        let bytes_per_word: i64 = 5;
        // words per minute. medium says 265-275, some other dude said 180.
        let wpm: f64 = 220.0;

        let prose_reading_time: f64 = (self.num_prose_bytes as f64 / bytes_per_word as f64) / wpm;

        let lines_of_code_per_minute = 10.0;
        let code_reading_time = self.num_code_lines as f64 / lines_of_code_per_minute;

        trace!(
            "path={}, computed reading time: {prose_reading_time:?} + {code_reading_time:?} = {}",
            self.args.path,
            prose_reading_time + code_reading_time
        );

        self.result.reading_time = (prose_reading_time + code_reading_time).ceil() as i64;
    }

    fn process_nested_markdown(&mut self, markdown: &Markdown) -> noteyre::Result<()> {
        trace!("\n>>>> Nested markdown starts...");

        let nested_parser = Parser::new_ext(markdown.as_str(), options());

        let mut nested_formatter = Formatter {
            stack: Vec::new(),
            footnote_counter: self.footnote_counter,
            footnotes: self.footnotes.clone(),

            args: ProcessMarkdownArgs {
                path: self.args.path,
                markdown,
                w: self.args.w,
                rv: self.args.rv.clone(),
                ti: self.args.ti.clone(),
                templates: self.args.templates,
                web: self.args.web,
            },
            result: Default::default(),

            highlight: self.highlight,
            math: self.math,
            media: self.media,

            mode: self.mode,

            discard_writer: DiscardWriter,

            num_prose_bytes: 0,
            num_code_lines: 0,
        };

        nested_formatter.drain_parser(nested_parser)?;

        self.footnote_counter += nested_formatter.footnote_counter;
        self.footnotes.extend(nested_formatter.footnotes);

        self.num_prose_bytes += nested_formatter.num_prose_bytes;
        self.num_code_lines += nested_formatter.num_code_lines;

        extend_markdown_result(&mut self.result, nested_formatter.result);

        trace!("<<< Nested markdown ends!\n");

        Ok(())
    }

    fn insert_shortcode_globals(&self, args: &mut DataObject) {
        args.insert(
            "__page_input_path".into(),
            self.args.path.to_string().into(),
        );
    }
}

fn yaml_to_data_object(yaml: &Yaml) -> noteyre::Result<DataObject> {
    let mut data_object = DataObject::new();

    if let Yaml::Null = yaml {
        // no args, that's ok
        return Ok(data_object);
    }

    if let Yaml::Hash(hash) = yaml {
        for (key, value) in hash {
            let key_str = match key {
                Yaml::String(s) => s,
                _ => bail!("Unsupported key type: {:?}", key),
            };

            let data_value = match value {
                Yaml::String(s) => DataValue::String(s.clone()),
                Yaml::Integer(i) => DataValue::Number(*i as i32),
                Yaml::Boolean(b) => DataValue::Boolean(*b),
                _ => bail!("Unsupported value type: {:?}", value),
            };

            data_object.insert(key_str.clone(), data_value);
        }
        Ok(data_object)
    } else {
        bail!("Expected a YAML Hash, got {:?}", yaml)
    }
}

struct DebugWrapper<'a, 'b>(&'a Event<'b>);

impl std::fmt::Debug for DebugWrapper<'_, '_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.0 {
            Event::Text(text) => write!(f, "Text({:?})", text as &str),
            other => write!(f, "{other:?}"),
        }
    }
}

pub(crate) struct DiscardWriter;

impl std::io::Write for DiscardWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn extend_markdown_result(receiver: &mut ProcessMarkdownResult, giver: ProcessMarkdownResult) {
    receiver.plain_text.push_str(&giver.plain_text);
    receiver.links.extend(giver.links);
    receiver.deps.extend(giver.deps);
}

impl ModImpl {
    pub(crate) fn mk_formatter<'a>(
        &self,
        mode: FormatterMode,
        args: ProcessMarkdownArgs<'a>,
    ) -> Formatter<'a> {
        Formatter {
            stack: Vec::new(),
            footnote_counter: 0,
            footnotes: HashMap::new(),

            args,
            result: Default::default(),

            highlight: self.highlight,
            math: self.math,
            media: self.media,

            mode,

            discard_writer: DiscardWriter,

            num_code_lines: 0,
            num_prose_bytes: 0,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use crate::{Mod, ModImpl};

    use super::*;
    use config::{Environment, TenantConfig, TenantInfo, WebConfig};
    use conflux::{MarkdownRef, RevisionView};
    use indoc::indoc;
    use template_types::{RenderShortcodeResult, RenderTemplateArgs, TemplateCollection};

    struct DummyHighlight;

    impl highlight::Mod for DummyHighlight {
        fn highlight_code(
            &self,
            mut w: &mut dyn std::io::Write,
            params: highlight::HighlightCodeParams<'_>,
        ) -> highlight::Result<()> {
            write!(
                w,
                "<code data-lang={:?} data-bo={}>",
                params.tag, params.byte_offset
            )?;
            html_escape::encode_safe_to_writer(params.source, &mut w)?;
            write!(w, "</code>")?;
            Ok(())
        }

        fn all_icons(&self) -> String {
            todo!()
        }
    }

    struct DummyMath;

    impl math::Mod for DummyMath {
        fn render_math(
            &self,
            input: &str,
            _mode: math::MathMode,
            w: &mut dyn std::io::Write,
        ) -> noteyre::Result<()> {
            write!(w, "<span class=\"mathml\">{}</span>", input)?;
            Ok(())
        }
    }

    struct DummyTemplateCollection;
    impl TemplateCollection for DummyTemplateCollection {
        fn render_template_to(
            &self,
            _w: &mut dyn std::io::Write,
            _args: RenderTemplateArgs<'_>,
        ) -> noteyre::Result<()> {
            todo!()
        }

        fn render_shortcode_to(
            &self,
            w: &mut dyn std::io::Write,
            args: template_types::Shortcode<'_>,
            _rv: Arc<dyn RevisionView>,
            _web: WebConfig,
        ) -> noteyre::Result<RenderShortcodeResult> {
            writeln!(w, "Before:")?;
            writeln!(w, "{}", args.body.unwrap_or_default())?;
            writeln!(w, "After (also, with args {:?})", args.args)?;
            Ok(RenderShortcodeResult {
                shortcode_input_path: InputPath::new("foobar.jinja".to_string()),
                assets_looked_up: Default::default(),
            })
        }
    }

    struct DummyMedia;

    impl media::Mod for DummyMedia {
        fn media_html_markup(&self, _opts: media::MediaMarkupOpts) -> noteyre::Result<String> {
            Ok("<img>".into())
        }
    }

    fn to_html(markdown: &MarkdownRef) -> (String, ProcessMarkdownResult) {
        let mut output = Vec::new();
        let mod_instance = ModImpl {
            highlight: &DummyHighlight,
            math: &DummyMath,
            media: &DummyMedia,
        };
        use config::camino::Utf8PathBuf;

        let result = mod_instance
            .process_markdown_to_writer(ProcessMarkdownArgs {
                path: InputPathRef::from_str("/content/dummy.md"),
                markdown,
                w: &mut output,
                ti: Arc::new(TenantInfo {
                    base_dir: Utf8PathBuf::from("/tmp/fasterthanli.me"),
                    tc: TenantConfig::new("fasterthanli.me".into()),
                }),
                rv: Arc::new(()),
                templates: &DummyTemplateCollection,
                web: WebConfig {
                    env: Environment::Development,
                    port: 1111,
                },
            })
            .unwrap();
        (String::from_utf8(output).unwrap(), result)
    }

    #[test]
    fn markdown_1() {
        let markdown = indoc! {r#"---
        title: Test Post
        date: 2023-01-01
        draft: true
        extra:
            author: test
        ---

        # Hello World

        This is a test.

        ## Subheading

        ### Subsubheading

        ## Now `with` some code

        And a paragraph.
        "#};

        let (html, result) = to_html(MarkdownRef::from_str(markdown));
        insta::assert_snapshot!(result.plain_text);
        insta::assert_snapshot!(result.frontmatter.unwrap());
        insta::assert_debug_snapshot!(result.toc);
        insta::assert_snapshot!(html);
    }

    #[test]
    fn markdown_with_figure() {
        let markdown = indoc! {r#"
        # Main Heading

        <figure>
        <img src="example.jpg">
        <figcaption>
        <h4>Image Title</h4>
        <a href="https://example.com">Attribution</a>
        </figcaption>
        </figure>

        ## Second Heading
        "#};

        let (html, result) = to_html(MarkdownRef::from_str(markdown));
        insta::assert_debug_snapshot!(result.toc);
        insta::assert_snapshot!(html);
    }

    #[test]
    fn markdown_with_code_block() {
        let markdown = indoc! {r#"
        # Code Example

        ```shell
        $ ls -lhA
        total 100M
        -rw-rw-r-- 1 amos amos 100M Jul  2 20:49 draw.io-21.4.0-windows-installer.exe
        ```

        ## After Code Block
        "#};

        let (html, result) = to_html(MarkdownRef::from_str(markdown));
        insta::assert_debug_snapshot!(result.toc);
        insta::assert_snapshot!(html);
    }

    #[test]
    fn correctness_with_types() {
        let markdown = include_str!("testdata/correctness-with-types.md");

        let (html, result) = to_html(MarkdownRef::from_str(markdown));
        insta::assert_debug_snapshot!(result.toc);
        insta::assert_snapshot!(html);
    }

    #[test]
    fn shortblocks() {
        let markdown = r#"
> *:bearsays*
>
> I am not sure about that
        "#;

        let (html, result) = to_html(MarkdownRef::from_str(markdown));
        insta::assert_debug_snapshot!(result.toc);
        insta::assert_snapshot!(html);
    }

    #[test]
    fn plaintext_links() {
        let markdown = r#"
The [Nature weekly journal of science](https://www.nature.com/nature-research/about) was
first published in 1869. And after one and a half century, it has finally completed one
cycle of [carcinization](https://en.wikipedia.org/wiki/Carcinisation), by publishing
an article about [the Rust programming language](https://www.rust-lang.org/).
        "#;

        let (_, result) = to_html(MarkdownRef::from_str(markdown));
        insta::assert_snapshot!(result.plain_text);
    }

    #[test]
    fn plaintext_punctuation() {
        let markdown = r#"
A disaster? I think not: for I have seen $100,000 to be made in this world.

There's not a "thing" I have not seen.

> Can you quote me on that?

Of course I can!

```rust
Not plain text
```

But also *this* is plain text. And **these are plain text too**.

How about a:

  * list
    * of
  * things
  * yes?
    * maybe
            "#;

        let (_, result) = to_html(MarkdownRef::from_str(markdown));
        insta::assert_snapshot!(result.plain_text);
    }

    #[test]
    fn toc_in_blockquote() {
        let markdown = indoc! {r#"
        # Main Heading

        > ## Heading in Blockquote
        >
        > This heading should not appear in the TOC.

        ## Second Main Heading

        > ### Another Blockquote Heading
        >
        > This one shouldn't be in the TOC either.

        ### Third Main Heading
        "#};

        let (_, result) = to_html(MarkdownRef::from_str(markdown));

        assert_eq!(result.toc.len(), 3);
        assert_eq!(result.toc[0].text, "Main Heading");
        assert_eq!(result.toc[1].text, "Second Main Heading");
        assert_eq!(result.toc[2].text, "Third Main Heading");
    }

    #[test]
    fn toc_in_shortcode() {
        let markdown = indoc! {r#"
        # Main Heading

        > *:shortcode*
        >
        > ## Heading in Shortcode
        >
        > This heading should not appear in the TOC.

        ## Second Main Heading

        > *:another_shortcode*
        >
        > ### Another Shortcode Heading
        >
        > This one shouldn't be in the TOC either.

        ### Third Main Heading
        "#};

        let (_, result) = to_html(MarkdownRef::from_str(markdown));

        assert_eq!(result.toc.len(), 3);
        assert_eq!(result.toc[0].text, "Main Heading");
        assert_eq!(result.toc[1].text, "Second Main Heading");
        assert_eq!(result.toc[2].text, "Third Main Heading");
    }

    #[test]
    fn home_is_very_opinionated() {
        let markdown = indoc! {r#"
        ## home is _very_ opinionated

        This is some content.
        "#};

        let (html, result) = to_html(MarkdownRef::from_str(markdown));
        insta::assert_snapshot!(html);
        insta::assert_snapshot!(result.plain_text);
    }
}
