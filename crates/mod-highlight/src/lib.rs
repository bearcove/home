include!(".dylo/spec.rs");
include!(".dylo/support.rs");

#[cfg(feature = "impl")]
const HIGHLIGHT_NAMES: &[&str] = &[
    "attribute",
    "constant",
    "function.builtin",
    "function",
    "keyword",
    "operator",
    "property",
    "punctuation",
    "punctuation.bracket",
    "punctuation.delimiter",
    "string",
    "string.special",
    "tag",
    "type",
    "type.builtin",
    "variable",
    "variable.builtin",
    "variable.parameter",
    "comment",
    "macro",
    "label",
    "diff.addition",
    "diff.deletion",
    // markdown_inline
    "number",
    "text.literal",
    "text.emphasis",
    "text.strong",
    "text.uri",
    "text.reference",
    "string.escape",
    // markdown
    "text.title",
    "punctuation.special",
    "text.strikethrough",
];

use std::{collections::HashMap, sync::Arc};

use tree_sitter_collection::tree_sitter_highlight::{
    self, Highlight, HighlightConfiguration, HighlightEvent, Highlighter,
};

#[cfg(feature = "impl")]
struct ModImpl {
    langs: HashMap<&'static str, Arc<Lang>>,
    cache: Option<dumbcache::Cache>,
}

#[cfg(feature = "impl")]
struct Lang {
    conf: Option<HighlightConfiguration>,
    name: &'static str,
    // see https://www.nerdfonts.com/cheat-sheet and `build.rs`
    icon: &'static str,
}

#[derive(Debug)]
pub enum Error {
    /// any error, tbh
    Any(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any(s) => write!(f, "{s}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(value: std::io::Error) -> Self {
        Self::Any(value.to_string())
    }
}

#[cfg(feature = "impl")]
impl From<tree_sitter_highlight::Error> for Error {
    fn from(value: tree_sitter_highlight::Error) -> Self {
        Self::Any(value.to_string())
    }
}

impl From<String> for Error {
    fn from(value: String) -> Self {
        Self::Any(value)
    }
}

impl From<&'static str> for Error {
    fn from(value: &'static str) -> Self {
        Self::Any(value.to_string())
    }
}

#[cfg(feature = "impl")]
impl From<std::str::Utf8Error> for Error {
    fn from(value: std::str::Utf8Error) -> Self {
        Self::Any(value.to_string())
    }
}

#[cfg(feature = "impl")]
impl From<std::string::FromUtf8Error> for Error {
    fn from(value: std::string::FromUtf8Error) -> Self {
        Self::Any(value.to_string())
    }
}

#[cfg(feature = "impl")]
impl From<tree_sitter_collection::tree_sitter::QueryError> for Error {
    fn from(value: tree_sitter_collection::tree_sitter::QueryError) -> Self {
        Self::Any(value.to_string())
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub struct HighlightCodeParams<'a> {
    /// the code to highlight
    pub source: &'a str,
    /// something like "rust" or "go" — whatever was
    /// in the fenced code block. it can be empty.
    pub tag: &'a str,
    /// written as `data-bo`
    pub byte_offset: usize,
}

#[dylo::export]
impl Mod for ModImpl {
    /// Get a string containing all nerdfont icons for all language types
    fn all_icons(&self) -> String {
        let mut icons = String::new();
        for lang in self.langs.values() {
            icons.push_str(lang.icon);
        }
        icons
    }

    fn highlight_code(
        &self,
        w: &mut dyn std::io::Write,
        params: HighlightCodeParams<'_>,
    ) -> Result<()> {
        use impls::*;

        let cache_key = format!("{}:::{}", params.tag, params.source);
        if let Some(cache) = &self.cache {
            if let Some(res) = cache.with(&cache_key, |output| {
                w.write_all(output.as_bytes())?;
                Ok(())
            }) {
                return res;
            }
        }

        tracing::trace!(
            "==== Highlighting code: {} bytes with tag {}",
            params.source.len(),
            params.tag
        );

        struct TeeWriter<'a> {
            inner: &'a mut dyn std::io::Write,
            buf: Vec<u8>,
        }

        impl<'a> TeeWriter<'a> {
            fn new(inner: &'a mut dyn std::io::Write) -> Self {
                Self {
                    inner,
                    buf: Default::default(),
                }
            }

            fn into_buf(self) -> Vec<u8> {
                self.buf
            }
        }

        impl std::io::Write for TeeWriter<'_> {
            fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
                let n = self.inner.write(buf)?;
                self.buf.extend_from_slice(&buf[..n]);
                Ok(n)
            }

            fn flush(&mut self) -> std::io::Result<()> {
                self.inner.flush()?;
                Ok(())
            }
        }

        let mut w = TeeWriter::new(w);

        {
            let w = &mut w;
            use std::io::Write;

            let lang = match self.langs.get(params.tag) {
                Some(lang) => lang,
                None => {
                    write_code_start(w, &params)?;
                    write_code_escaped(w, params.source)?;
                    write_code_end(w)?;
                    return Ok(());
                }
            };

            if lang.name == TERMINAL_LANG_NAME {
                // just let HTML-ified ANSI codes through
                write_code_start(w, &params)?;
                write!(w, "{}", params.source).unwrap();
                write_code_end(w)?;
                return Ok(());
            }

            let conf = match lang.conf.as_ref() {
                Some(conf) => conf,
                None => {
                    write_code_start(w, &params)?;
                    write_code_escaped(w, params.source)?;
                    write_code_end(w)?;
                    return Ok(());
                }
            };

            let mut highlighter = Highlighter::new();
            let highlights =
                highlighter.highlight(conf, params.source.as_bytes(), None, |lang_name| {
                    let res = self
                        .langs
                        .get(lang_name)
                        .and_then(|lang| lang.conf.as_ref());
                    match &res {
                        Some(_) => tracing::trace!("💉 Injecting {lang_name}"),
                        None => tracing::trace!("No language found for {lang_name} injection"),
                    }
                    res
                })?;

            write_code_start(w, &params)?;
            for highlight in highlights {
                let highlight = highlight.unwrap();
                match highlight {
                    HighlightEvent::Source { start, end } => {
                        tracing::trace!("Escaping code from {start} to {end}");
                        write_code_escaped(w, &params.source[start..end]).unwrap();
                    }
                    HighlightEvent::HighlightStart(Highlight(i)) => {
                        tracing::trace!("Starting highlight {} (.hh{i})", HIGHLIGHT_NAMES[i]);
                        write!(w, r#"<i class=hh{i}>"#).unwrap();
                    }
                    HighlightEvent::HighlightEnd => {
                        tracing::trace!("Ending highlight");
                        write!(w, r#"</i>"#).unwrap();
                    }
                }
            }
            write_code_end(w)?;
        }

        if let Some(cache) = &self.cache {
            let buf = w.into_buf();
            cache.insert(cache_key, String::from_utf8(buf)?);
        }

        Ok(())
    }
}

#[cfg(feature = "impl")]
impl Default for ModImpl {
    fn default() -> Self {
        let highlight_names = HIGHLIGHT_NAMES
            .iter()
            .cloned()
            .map(String::from)
            .collect::<Vec<_>>();
        let mut seen = std::collections::HashSet::new();
        for name in &highlight_names {
            if !seen.insert(name) {
                panic!("Duplicate highlight name: {name}");
            }
        }

        let mut res = Self {
            langs: Default::default(),
            cache: if std::env::var("MOD_HIGHLIGHT_NO_CACHE").is_err() {
                Some(dumbcache::Cache::new("highlight", 1024))
            } else {
                tracing::warn!("highlighting cache disabled");
                None
            },
        };

        {
            let mut conf = tree_sitter_collection::go().expect("failed to load go grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "go",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Go",
                    icon: nerdfonts::NF_SETI_GO2,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::c().expect("failed to load c grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "c",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "C",
                    icon: nerdfonts::NF_CUSTOM_C,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::cpp().expect("failed to load cpp grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "cpp",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "C++",
                    icon: nerdfonts::NF_CUSTOM_CPP,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::rust().expect("failed to load rust grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "rust",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Rust",
                    icon: nerdfonts::NF_SETI_RUST,
                }),
            );
        }
        {
            let mut conf =
                tree_sitter_collection::javascript().expect("failed to load javascript grammar");
            conf.configure(&highlight_names);

            let lang = Arc::new(Lang {
                conf: Some(conf),
                name: "JavaScript",
                icon: nerdfonts::NF_DEV_JAVASCRIPT,
            });
            res.langs.insert("javascript", lang.clone());
            res.langs.insert("js", lang);
        }
        {
            let mut conf =
                tree_sitter_collection::typescript().expect("failed to load typescript grammar");
            conf.configure(&highlight_names);
            let lang = Arc::new(Lang {
                conf: Some(conf),
                name: "TypeScript",
                icon: nerdfonts::NF_DEV_TYPESCRIPT,
            });
            res.langs.insert("typescript", lang.clone());
            res.langs.insert("ts", lang);
        }
        {
            let mut conf = tree_sitter_collection::tsx().expect("failed to load tsx grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "tsx",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "TypeScript React",
                    icon: nerdfonts::NF_DEV_TYPESCRIPT,
                }),
            );
        }
        {
            let mut conf =
                tree_sitter_collection::javascript().expect("failed to load javascript grammar");
            conf.configure(&highlight_names);

            res.langs.insert(
                "json",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "JSON",
                    icon: nerdfonts::NF_SETI_JSON,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::java().expect("failed to load java grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "java",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Java",
                    icon: nerdfonts::NF_FA_JAVA,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::toml().expect("failed to load toml grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "toml",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "TOML markup",
                    icon: nerdfonts::NF_CUSTOM_TOML,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::bash().expect("failed to load bash grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "bash",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Bash",
                    icon: nerdfonts::NF_MD_BASH,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::html().expect("failed to load html grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "html",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "HTML",
                    icon: nerdfonts::NF_SETI_HTML,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::html().expect("failed to load html grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "xml",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "XML",
                    icon: nerdfonts::NF_MD_XML,
                }),
            );
        }
        {
            res.langs.insert(
                "shell",
                Arc::new(Lang {
                    conf: None,
                    name: "Shell session",
                    icon: nerdfonts::NF_SETI_SHELL,
                }),
            );
        }
        {
            res.langs.insert(
                "pwsh",
                Arc::new(Lang {
                    conf: None,
                    name: "PowerShell session",
                    icon: nerdfonts::NF_SETI_POWERSHELL,
                }),
            );
        }
        {
            res.langs.insert(
                "pwsh-script",
                Arc::new(Lang {
                    conf: None,
                    name: "PowerShell script",
                    icon: nerdfonts::NF_SETI_POWERSHELL,
                }),
            );
        }
        {
            res.langs.insert(
                "term",
                Arc::new(Lang {
                    conf: None,
                    name: impls::TERMINAL_LANG_NAME,
                    icon: nerdfonts::NF_DEV_TERMINAL,
                }),
            );
        }
        {
            res.langs.insert(
                "raw",
                Arc::new(Lang {
                    conf: None,
                    name: "",
                    icon: nerdfonts::NF_FA_FILE_TEXT,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::python().expect("failed to load python grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "python",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Python",
                    icon: nerdfonts::NF_DEV_PYTHON,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::ini().expect("failed to load ini grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "meson-wrap",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "meson .wrap file",
                    icon: nerdfonts::NF_FA_CODE,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::meson().expect("failed to load meson grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "meson",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "meson.build file",
                    icon: nerdfonts::NF_FA_CODE,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::x86asm().expect("failed to load x86asm grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "x86asm",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "x86 assembly",
                    icon: nerdfonts::NF_CUSTOM_ASM,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::asm().expect("failed to load asm grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "asm",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Assembly",
                    icon: nerdfonts::NF_CUSTOM_ASM,
                }),
            );
        }
        {
            let mut c = tree_sitter_collection::yaml().expect("failed to load yaml grammar");
            c.configure(&highlight_names);
            let lang = Arc::new(Lang {
                conf: Some(c),
                name: "YAML",
                icon: nerdfonts::NF_SETI_YML,
            });
            res.langs.insert("yml", lang.clone());
            res.langs.insert("yaml", lang);
        }
        {
            let mut conf =
                tree_sitter_collection::dockerfile().expect("failed to load dockerfile grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "Dockerfile",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Dockerfile",
                    icon: nerdfonts::NF_FA_DOCKER,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::nix().expect("failed to load nix grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "nix",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "nix",
                    icon: nerdfonts::NF_MD_NIX,
                }),
            );
        }
        {
            let mut conf =
                tree_sitter_collection::clojure().expect("failed to load clojure grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "commonlisp",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Common Lisp",
                    icon: nerdfonts::NF_CUSTOM_COMMON_LISP,
                }),
            );

            let mut conf =
                tree_sitter_collection::clojure().expect("failed to load clojure grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "emacslisp",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Emacs Lisp",
                    icon: nerdfonts::NF_CUSTOM_COMMON_LISP,
                }),
            );

            let mut conf =
                tree_sitter_collection::clojure().expect("failed to load clojure grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "clojure",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Clojure",
                    icon: nerdfonts::NF_DEV_CLOJURE_ALT,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::zig().expect("failed to load zig grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "zig",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Zig",
                    icon: nerdfonts::NF_SETI_ZIG,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::diff().expect("failed to load diff grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "diff",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Diff",
                    icon: nerdfonts::NF_COD_DIFF,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::css().expect("failed to load css grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "css",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "CSS",
                    icon: nerdfonts::NF_SETI_CSS,
                }),
            );
        }
        {
            let mut conf = tree_sitter_collection::jinja().expect("failed to load jinja grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "jinja",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Jinja",
                    icon: nerdfonts::NF_FA_FLASK,
                }),
            );
        }
        {
            let mut conf =
                tree_sitter_collection::markdown().expect("failed to load markdown grammar");
            conf.configure(&highlight_names);
            let lang = Arc::new(Lang {
                conf: Some(conf),
                name: "Markdown",
                icon: nerdfonts::NF_DEV_MARKDOWN,
            });
            res.langs.insert("markdown", lang.clone());
            res.langs.insert("md", lang);
        }

        {
            let mut conf = tree_sitter_collection::markdown_inline()
                .expect("failed to load markdown_inline grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "markdown_inline",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "Markdown Inline",
                    icon: nerdfonts::NF_DEV_MARKDOWN,
                }),
            );
        }

        {
            let mut conf = tree_sitter_collection::latex().expect("failed to load latex grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "latex",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "LaTeX",
                    icon: nerdfonts::NF_DEV_LATEX,
                }),
            );
        }

        {
            let mut conf = tree_sitter_collection::scss().expect("failed to load scss grammar");
            conf.configure(&highlight_names);
            res.langs.insert(
                "scss",
                Arc::new(Lang {
                    conf: Some(conf),
                    name: "SCSS",
                    icon: nerdfonts::NF_DEV_SASS,
                }),
            );
        }

        res
    }
}

#[cfg(feature = "impl")]
pub(crate) mod impls {
    use crate::HighlightCodeParams;
    use crate::Lang;
    use crate::Result;

    use std::io;

    pub(crate) const TERMINAL_LANG_NAME: &str = "Terminal";

    pub(crate) fn write_code_start(
        w: &mut dyn io::Write,
        params: &HighlightCodeParams,
    ) -> Result<()> {
        let lang = match params.tag {
            "term" => Some(Lang {
                conf: None,
                name: TERMINAL_LANG_NAME,
                icon: nerdfonts::NF_DEV_TERMINAL,
            }),
            _ => None,
        };

        let name = lang.as_ref().map(|l| l.name).unwrap_or_default();

        let has_language_tag = !name.is_empty();
        let lang_desc = if has_language_tag {
            format!("{} code block", name)
        } else {
            "Code block".to_string()
        };

        write!(
            w,
            r#"<figure role="region" aria-label="{lang_desc}" class="code-block"#
        )?;
        if has_language_tag {
            write!(w, r#" has-language-tag"#)?;
        }
        if params.tag == "term" {
            write!(w, r#" home-ansi"#)?;
        }
        write!(
            w,
            r#"" translate="no" data-lang={:?} data-bo="{}">"#,
            params.tag, params.byte_offset
        )?;

        if let Some(lang) = &lang {
            let icon = &lang.icon;
            write!(
                w,
                r#"<span class="language-tag" title="{name}">{icon}</span>"#
            )?;
        }

        write!(w, r#"<code class="scroll-wrapper">"#)?;
        Ok(())
    }

    pub(crate) fn write_code_end(w: &mut dyn io::Write) -> Result<()> {
        write!(w, "</code></figure>")?;
        Ok(())
    }

    pub(crate) fn write_code_escaped(w: &mut dyn io::Write, input: &str) -> Result<()> {
        let mut start: Option<usize> = None;

        tracing::trace!("Escaping code: {input:?}");

        for (i, c) in input.char_indices() {
            match c {
                '<' | '>' | '&' => {
                    if let Some(start) = start.take() {
                        write!(w, "{}", &input[start..i])?;
                    }
                    match c {
                        '<' => write!(w, "&lt;")?,
                        '>' => write!(w, "&gt;")?,
                        '&' => write!(w, "&amp;")?,
                        _ => {}
                    };
                }
                _ => {
                    if start.is_none() {
                        start = Some(i)
                    }
                }
            }
        }
        if let Some(start) = start.take() {
            write!(w, "{}", &input[start..])?;
        }

        Ok(())
    }

    #[cfg(all(test, feature = "impl"))]
    mod tests {
        use super::*;

        #[test]
        fn test_write_code_escaped() {
            let mut out = Vec::new();
            write_code_escaped(&mut out, "The Vec<u8> type").unwrap();
            assert_eq!(std::str::from_utf8(&out).unwrap(), "The Vec&lt;u8&gt; type");

            out.clear();
            write_code_escaped(&mut out, "ParseResult<&str> Or Result<Vec<_>> && false").unwrap();
            assert_eq!(
                std::str::from_utf8(&out).unwrap(),
                "ParseResult&lt;&amp;str&gt; Or Result&lt;Vec&lt;_&gt;&gt; &amp;&amp; false"
            );
        }
    }
}
