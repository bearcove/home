include!(".dylo/spec.rs");
include!(".dylo/support.rs");

use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

#[cfg(feature = "impl")]
use std::sync::Mutex;

#[cfg(feature = "impl")]
mod impls;

#[cfg(feature = "impl")]
use closest::{GetOrHelp, ResourceKind};
use config::WebConfig;
use conflux::{InputPath, LoadedPage, RevisionView, RouteRef};
use credentials::UserInfo;
use mom::GlobalStateView;
#[cfg(feature = "impl")]
use noteyre::BsForResults;
use noteyre::eyre;

#[cfg(feature = "impl")]
use minijinja::{Environment, Value};
use search::Index;
#[cfg(feature = "impl")]
use template_types::GlobalsVal;

#[cfg(feature = "impl")]
mod conversions;

#[cfg(feature = "impl")]
mod global_functions_and_filters;
#[cfg(feature = "impl")]
use global_functions_and_filters::truncate_core;

#[cfg(feature = "impl")]
mod prettify_minijinja_errors;
#[cfg(feature = "impl")]
use prettify_minijinja_errors::PrettifyExt;
#[cfg(feature = "impl")]
use template_types::RevisionViewHolder;

#[cfg(feature = "impl")]
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

#[cfg(feature = "impl")]
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

#[derive(PartialEq, Eq, Debug)]
pub struct Shortcode<'a> {
    pub name: &'a str,
    pub body: Option<&'a str>,
    pub args: DataObject,
}

pub fn shortcode_name_to_input_path(name: &str) -> InputPath {
    format!("/templates/shortcodes/{name}.html.jinja").into()
}

pub struct RenderShortcodeResult {
    /// for dependency tracking
    pub shortcode_input_path: InputPath,

    /// for dependency tracking
    pub assets_looked_up: HashSet<InputPath>,
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

#[cfg(feature = "impl")]
mod template_types;
