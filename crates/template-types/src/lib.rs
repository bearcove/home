use std::sync::Arc;

use conflux::{LoadedPage, RevisionView, RouteRef};

pub trait TemplateCollection {
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
    pub user_info: Option<UserInfo<'static>>,

    /// Web configuration
    pub web: WebConfig,

    /// Additional globals
    pub additional_globals: DataObject,
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
