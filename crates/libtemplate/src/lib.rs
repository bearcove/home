use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

use std::sync::Mutex;

mod impls;

use closest::{GetOrHelp, ResourceKind};
use config::WebConfig;
use conflux::{InputPath, LoadedPage, RevisionView, RouteRef};
use credentials::UserInfo;
use mom::GlobalStateView;

use eyre::eyre;
use minijinja::{Environment, Value};
use search::Index;
use template_types::GlobalsVal;

mod conversions;

mod global_functions_and_filters;
use global_functions_and_filters::truncate_core;

mod prettify_minijinja_errors;
use prettify_minijinja_errors::PrettifyExt;
use template_types::RevisionViewHolder;

#[derive(Default)]
struct ModImpl;

#[dylo::export]
impl Mod for ModImpl {
    fn make_collection(&self, args: CompileArgs) -> noteyre::Result<Box<dyn TemplateCollection>> {
        let mut environment = Environment::new();

        environment.set_debug(true);

        environment.set_loader(move |path| {
            Ok(Some(
                args.templates
                    .get_or_help(ResourceKind::Template, path)
                    .map_err(|e| {
                        eprintln!("on template lookup: {e}");
                        minijinja::Error::new(minijinja::ErrorKind::TemplateNotFound, e.to_string())
                    })?
                    .clone(),
            ))
        });

        global_functions_and_filters::register_all(&mut environment);

        Ok(Box::new(TemplateCollectionImpl { environment }))
    }
}

#[cfg(feature = "impl")]
struct TemplateCollectionImpl {
    environment: Environment<'static>,
}

#[dylo::export]
impl TemplateCollection for TemplateCollectionImpl {
    fn render_template_to(
        &self,
        w: &mut dyn std::io::Write,
        args: RenderTemplateArgs<'_>,
    ) -> noteyre::Result<()> {
        let template_name = args.template_name;
        let template = self
            .environment
            .get_template(template_name)
            .prettify_minijinja_error()?;

        let globals = GlobalsVal {
            page: args.page.clone(),
            user_info: args.user_info,
            additional_globals: args.additional_globals,
            raw_query: args.raw_query.to_owned(),
            url_params: form_urlencoded::parse(args.raw_query.as_bytes())
                .map(|(k, v)| (k.into_owned(), v.into_owned()))
                .collect(),
            rv: args.rv,
            gv: args.gv,
            index: args.index,
            web: args.web,
        };

        template
            .render_to_write(Value::from_object(globals), w)
            .prettify_minijinja_error()?;
        Ok(())
    }

    fn render_shortcode_to(
        &self,
        w: &mut dyn std::io::Write,
        sc: Shortcode<'_>,
        rv: Arc<dyn RevisionView>,
        web: WebConfig,
    ) -> noteyre::Result<RenderShortcodeResult> {
        let template_name = format!("shortcodes/{}.html", sc.name);
        let template_input_path = InputPath::new(format!("/templates/{template_name}.jinja"));
        let template_input = rv.rev().bs()?.inputs().get(&template_input_path).cloned().ok_or_else(|| {
                eyre!("shortcode template not found: {template_name}, tried input path {template_input_path}")
            })?;

        let cachebusted_deps: Arc<Mutex<HashSet<InputPath>>> = Arc::new(Mutex::new(HashSet::new()));
        let rv = Arc::new(impls::TrackingRevisionView::new(
            rv,
            cachebusted_deps.clone(),
        ));

        let mut args: HashMap<String, Value> = sc
            .args
            .into_iter()
            .map(|(k, v)| {
                (
                    k,
                    match v {
                        DataValue::String(s) => Value::from(s),
                        DataValue::Number(n) => Value::from(n),
                        DataValue::Boolean(b) => Value::from(b),
                    },
                )
            })
            .collect();
        args.insert(
            "__revision_view".into(),
            Value::from_object(RevisionViewHolder(rv)),
        );
        args.insert("web_port".into(), web.port.into());

        if let Some(body) = sc.body {
            args.insert("body".into(), body.into());
        }

        let template = self
            .environment
            .get_template(&template_name)
            .prettify_minijinja_error()?;

        template.render_to_write(Value::from_object(args), w).bs()?;
        Ok(RenderShortcodeResult {
            shortcode_input_path: template_input.path.clone(),
            assets_looked_up: {
                let guard = cachebusted_deps.lock().unwrap();
                guard.clone()
            },
        })
    }
}

#[derive(Default)]
pub struct CompileArgs {
    pub templates: HashMap<String, String>,
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

pub fn shortcode_name_to_input_path(name: &str) -> InputPath {
    format!("/templates/shortcodes/{name}.html.jinja").into()
}

impl TemplateCollection for () {
    fn render_template_to(
        &self,
        _w: &mut dyn std::io::Write,
        _args: RenderTemplateArgs<'_>,
    ) -> noteyre::Result<()> {
        Err(eyre!("no template collection"))
    }

    fn render_shortcode_to(
        &self,
        _w: &mut dyn std::io::Write,
        _args: Shortcode<'_>,
        _rv: Arc<dyn RevisionView>,
        _web: WebConfig,
    ) -> noteyre::Result<RenderShortcodeResult> {
        Err(eyre!("no template collection"))
    }
}

mod template_types;
