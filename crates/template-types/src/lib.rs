use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use config_types::WebConfig;
use conflux::{InputPath, LoadedPage, RevisionView, RouteRef};
use credentials::UserInfo;
use libsearch::Index;
use mom_types::GlobalStateView;

pub trait TemplateCollection: Send + Sync {
    fn render_template_to(
        &self,
        w: &mut dyn std::io::Write,
        args: RenderTemplateArgs<'_>,
    ) -> eyre::Result<()>;

    fn render_shortcode_to(
        &self,
        w: &mut dyn std::io::Write,
        sc: Shortcode<'_>,
        rv: Arc<dyn RevisionView>,
        web: WebConfig,
    ) -> eyre::Result<RenderShortcodeResult>;
}

impl TemplateCollection for () {
    fn render_template_to(
        &self,
        _w: &mut dyn std::io::Write,
        _args: RenderTemplateArgs<'_>,
    ) -> eyre::Result<()> {
        Err(eyre::eyre!("no template collection"))
    }

    fn render_shortcode_to(
        &self,
        _w: &mut dyn std::io::Write,
        _args: Shortcode<'_>,
        _rv: Arc<dyn RevisionView>,
        _web: WebConfig,
    ) -> eyre::Result<RenderShortcodeResult> {
        Err(eyre::eyre!("no template collection"))
    }
}

pub struct RenderTemplateArgs<'a> {
    pub template_name: &'a str,

    /// URL structure:
    /// +-------------------------+---+-------------------+
    /// | /articles/foo           | ? | bar=baz&qux=quux  |
    /// +-------------------------+---+-------------------+
    /// | Path                    |   | Raw Query         |
    /// +-------------------------+---+-------------------+
    pub path: &'a RouteRef,
    pub raw_query: &'a str,

    /// Revision bundle
    pub rv: Arc<dyn RevisionView>,

    /// Global state view
    pub gv: Arc<dyn GlobalStateView>,

    /// Search index
    pub index: Arc<dyn Index>,

    /// Page we're rendering (optional)
    pub page: Option<Arc<LoadedPage>>,

    /// Gotten from cookies
    pub user_info: Option<UserInfo>,

    /// Web configuration
    pub web: WebConfig,

    /// Additional globals
    pub additional_globals: DataObject,
}

#[derive(PartialEq, Eq, Debug)]
pub struct Shortcode<'a> {
    pub name: &'a str,
    pub body: Option<&'a str>,
    pub args: DataObject,
}

pub type DataObject = HashMap<String, DataValue>;

#[derive(PartialEq, Eq, Debug)]
pub enum DataValue {
    String(String),
    Number(i32),
    Boolean(bool),
}

impl<'a> From<&'a str> for DataValue {
    fn from(s: &'a str) -> Self {
        Self::String(s.to_owned())
    }
}

impl From<String> for DataValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<i32> for DataValue {
    fn from(n: i32) -> Self {
        Self::Number(n)
    }
}

impl From<bool> for DataValue {
    fn from(b: bool) -> Self {
        Self::Boolean(b)
    }
}

pub struct RenderShortcodeResult {
    /// for dependency tracking
    pub shortcode_input_path: InputPath,

    /// for dependency tracking
    pub assets_looked_up: HashSet<InputPath>,
}

#[derive(Default)]
pub struct CompileArgs {
    pub templates: HashMap<String, String>,
}
