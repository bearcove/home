use std::sync::Arc;

use closest::{GetOrHelp, ResourceKind};
use config_types::WebConfig;
use conflux::{InputPath, InputPathRef, RevisionView, RouteRef, Viewer};
use itertools::Itertools;
use minijinja::{Environment, Error, Value, value::Kwargs};
use rand::seq::SliceRandom;
use time::OffsetDateTime;

use crate::{
    GlobalsVal, Listing, ListingKind, LoadedPageVal, MediaVal, RevisionViewHolder,
    SearchResultsVal, ToVal, conversions::ToMinijinaResult,
};

fn urlencode(input: String) -> String {
    urlencoding::encode(&input).to_string()
}

use percent_encoding::{AsciiSet, CONTROLS, percent_encode};

// Define the custom encode set for fragments, encoding '-' and '.'
// Standard fragment encoding doesn't encode these, but some contexts might require it.
// See: https://url.spec.whatwg.org/#fragment-percent-encode-set
// The default fragment encode set includes controls, space, ", <, >, `
const FRAGMENT_ENCODE_SET_BASE: &AsciiSet =
    &CONTROLS.add(b' ').add(b'"').add(b'<').add(b'>').add(b'`');

// Add '-' and '.' to the base set for our custom fragment encoding
const CUSTOM_FRAGMENT_ENCODE_SET: &AsciiSet = &FRAGMENT_ENCODE_SET_BASE.add(b'-').add(b'.');

/// Encodes a string for use in a URL fragment (#fragment), with additional
/// encoding for '-' and '.' characters.
fn fragment_urlencode(input: String) -> String {
    percent_encode(input.as_bytes(), CUSTOM_FRAGMENT_ENCODE_SET).to_string()
}

fn shuffle(mut input: Vec<Value>) -> Result<Vec<Value>, Error> {
    input.shuffle(&mut rand::thread_rng());
    Ok(input)
}

pub fn get_revision_view(state: &minijinja::State) -> Arc<dyn RevisionView> {
    let rv_holder = state
        .lookup("__revision_view")
        .expect("should find __revision_view in state");
    let rv_holder = rv_holder
        .downcast_object_ref::<RevisionViewHolder>()
        .expect("should be RevisionViewHolder");
    rv_holder.0.clone()
}

pub fn get_globals(state: &minijinja::State) -> Result<Arc<GlobalsVal>, Error> {
    state
        .lookup("globals")
        .and_then(|v| v.downcast_object::<GlobalsVal>())
        .ok_or_else(|| Error::new(minijinja::ErrorKind::InvalidOperation, "globals not found"))
}

pub fn get_web_config(state: &minijinja::State) -> Result<WebConfig, Error> {
    let port = state
        .lookup("web_port")
        .and_then(|v| v.as_i64())
        .map(|v| v as u16)
        .ok_or_else(|| {
            Error::new(
                minijinja::ErrorKind::InvalidOperation,
                "web_port not found or not a number",
            )
        })?;
    let env = config_types::Environment::default();
    Ok(WebConfig { port, env })
}

fn asset_url(state: &minijinja::State, mut path: InputPath) -> Result<Value, Error> {
    let is_http_or_https =
        path.as_str().starts_with("http://") || path.as_str().starts_with("https://");
    if is_http_or_https {
        return Ok(Value::from_safe_string(path.as_str().to_string()));
    }

    if let Some(page_input_path) = state.lookup("__page_input_path") {
        // this lets markdown pages pass relative asset paths to shortcodes,
        // instead of having to specify `/content/articles/foobar/assets/blah.png`,
        // a page at `/content/articles/foobar/_index.md` can just use `assets/blah.png`
        //
        // this propagates all the way to shortcodes, which are able to call asset_url
        // and have the path be "canonicalized" here, relative to the input path of the
        // markdown page that invoked the shortcode.
        let page_input_path = InputPathRef::from_str(page_input_path.as_str().unwrap_or_default());
        path = page_input_path.canonicalize_relative_path(&path);
    }

    let res = get_revision_view(state)
        .cachebuster()
        .asset_url(get_web_config(state)?, &path);
    Ok(res
        .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))?
        .into())
}

fn get_media(state: &minijinja::State, mut path: InputPath) -> Result<Value, Error> {
    if path.as_str().starts_with("http://") || path.as_str().starts_with("https://") {
        return Err(Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("No hotlinking allowed: {}", path.as_str()),
        ));
    }

    if let Some(page_input_path) = state.lookup("__page_input_path") {
        // this lets markdown pages pass relative asset paths to shortcodes,
        // instead of having to specify `/content/articles/foobar/assets/blah.png`,
        // a page at `/content/articles/foobar/_index.md` can just use `assets/blah.png`
        //
        // this propagates all the way to shortcodes, which are able to call asset_url
        // and have the path be "canonicalized" here, relative to the input path of the
        // markdown page that invoked the shortcode.
        let page_input_path = InputPathRef::from_str(page_input_path.as_str().unwrap_or_default());
        path = page_input_path.canonicalize_relative_path(&path);
    }

    let rv = get_revision_view(state);
    let media = rv.cachebuster().media(&path).map_err(|e| {
        Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("media not found: {e}"),
        )
    })?;
    Ok(Value::from(MediaVal {
        path,
        media: media.clone(),
        web: get_web_config(state)?,
    }))
}

fn get_page_from_path(state: &minijinja::State, path: String) -> Result<Value, Error> {
    let rv = get_revision_view(state);
    let rev = rv.rev().map_err(|e| {
        Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("revision error: {e}"),
        )
    })?;
    let path = InputPathRef::from_str(&path);
    let page = rev
        .pages
        .get_or_help(ResourceKind::Page, path)
        .map_err(|e| {
            Error::new(
                minijinja::ErrorKind::InvalidOperation,
                format!("page not found: {e}"),
            )
        })?;
    Ok(page.clone().to_val())
}

fn get_page_from_route(state: &minijinja::State, path: String) -> Result<Value, Error> {
    let rv = get_revision_view(state);
    let rev = rv.rev().mj()?;
    let path = rev
        .page_routes
        .get_or_help(ResourceKind::Route, RouteRef::from_str(&path))
        .mj()?;
    let page = rev.pages.get_or_help(ResourceKind::Page, path).mj()?;
    let page = page.clone().to_val();
    Ok(page)
}

// This is used to generate the RSS feed
fn get_recent_pages(state: &minijinja::State) -> Result<Value, Error> {
    let viewer = Viewer {
        is_admin: false,
        has_bronze: false,
        has_silver: false,
    };

    // pages that are article or series_part, and listed, sorted by date descending,
    // limit to 25 items
    let rv = get_revision_view(state);
    let pages = rv
        .rev()
        .mj()?
        .pages
        .values()
        .filter(|p| p.is_article() || p.is_series_part())
        .filter(|p| p.is_listed(&viewer))
        .sorted_by_key(|p| p.date)
        .rev()
        .take(25)
        .cloned()
        .map(|p| p.to_val())
        .collect::<Vec<_>>();
    Ok(Value::from(pages))
}

fn url_encode(value: String) -> Result<String, Error> {
    Ok(urlencoding::encode(&value).into_owned())
}

fn html_escape(value: String) -> Result<Value, Error> {
    let mut value_escaped = String::new();
    html_escape::encode_safe_to_string(&value, &mut value_escaped);
    Ok(Value::from_safe_string(value_escaped))
}

fn truncate_html(html: String, args: Kwargs) -> Result<String, Error> {
    let max = args.get::<u64>("max").unwrap_or(300);
    args.assert_all_used()?;
    Ok(htmlrewrite::load().truncate_html(&html, max))
}

pub(crate) fn truncate_core(input: &str, len: usize) -> String {
    if input.chars().count() <= len {
        input.to_string()
    } else {
        let mut truncated = input
            .chars()
            .take(len.saturating_sub(3))
            .collect::<String>();
        truncated.push_str("...");
        truncated
    }
}

fn truncate(input: String, args: Kwargs) -> Result<String, Error> {
    let len = args.get::<usize>("len")?;
    args.assert_all_used()?;
    Ok(truncate_core(&input, len))
}

fn downcase(input: String) -> Result<String, Error> {
    Ok(input.to_lowercase())
}

fn to_json(value: Value) -> Result<String, Error> {
    // TODO: impl merde's `Serialize` for a wrapper of `Value` I guess.
    serde_json::to_string_pretty(&value).map_err(|e| {
        Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("Failed to serialize JSON: {}", e),
        )
    })
}

fn unknown_method_callback(
    _state: &minijinja::State,
    value: &Value,
    method: &str,
    args: &[Value],
) -> Result<Value, Error> {
    use minijinja::value::from_args;

    if let Some(s) = value.as_str() {
        match method {
            "endswith" => {
                let (suffix,): (&str,) = from_args(args)?;
                return Ok(Value::from(s.ends_with(suffix)));
            }
            "startswith" => {
                let (prefix,): (&str,) = from_args(args)?;
                return Ok(Value::from(s.starts_with(prefix)));
            }
            _ => {}
        }
    }

    Err(Error::from(minijinja::ErrorKind::UnknownMethod))
}

fn all_icons() -> Result<Value, Error> {
    Ok(Value::from(highlight::load().all_icons()))
}

const DAY_MONTH_YEAR_FORMAT: &[time::format_description::FormatItem<'static>] =
    time::macros::format_description!("[month repr:short] [day], [year]");

const MONTH_YEAR_FORMAT: &[time::format_description::FormatItem<'static>] =
    time::macros::format_description!("[month repr:long] [year]");

fn parse_datetime(input: &str) -> Result<OffsetDateTime, Error> {
    OffsetDateTime::parse(input, &time::format_description::well_known::Rfc3339)
        .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))
}

pub fn format_day_month_year(input: String) -> Result<String, Error> {
    let dt = parse_datetime(&input)?;
    dt.format(DAY_MONTH_YEAR_FORMAT)
        .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))
}

pub fn format_month_year(input: String) -> Result<String, Error> {
    let dt = parse_datetime(&input)?;
    dt.format(MONTH_YEAR_FORMAT)
        .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))
}

pub fn format_time_ago(input: String) -> Result<String, Error> {
    let dt = parse_datetime(&input)?;
    let now = OffsetDateTime::now_utc();
    let duration = now - dt;
    let days = duration.whole_days();

    fn pluralize(count: i64, singular: &str, plural: &str) -> String {
        if count == 1 {
            format!("1 {singular}")
        } else {
            format!("{count} {plural}")
        }
    }

    if days < 1 {
        Ok("today".to_string())
    } else if days < 7 {
        Ok(format!("{} ago", pluralize(days, "day", "days")))
    } else if days < 30 {
        let weeks = (days as f64 / 7.0).round() as i64;
        Ok(format!("{} ago", pluralize(weeks, "week", "weeks")))
    } else if days < 365 {
        let months = (days as f64 / 30.0).round() as i64;
        Ok(format!("{} ago", pluralize(months, "month", "months")))
    } else {
        let years = (days as f64 / 365.0).round() as i64;
        Ok(format!("{} ago", pluralize(years, "year", "years")))
    }
}

pub fn format_rfc3339(input: String) -> Result<String, Error> {
    let dt = parse_datetime(&input)?;
    dt.format(&time::format_description::well_known::Rfc3339)
        .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))
}

pub fn is_future(input: String) -> Result<bool, Error> {
    let dt = parse_datetime(&input)?;
    Ok(dt > OffsetDateTime::now_utc())
}

fn basic_markdown(input: String) -> Result<String, Error> {
    markdown::load().basic_markdown(&input).map_err(|e| {
        Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("markdown error: {e}"),
        )
    })
}

fn escape_for_attribute(input: String) -> Result<String, Error> {
    let escaped = input.replace('\n', " ").replace('"', "'");
    Ok(escaped)
}

fn random_article(state: &minijinja::State) -> Result<Value, Error> {
    let viewer = Viewer {
        is_admin: false,
        has_bronze: false,
        has_silver: false,
    };

    let rv = get_revision_view(state);
    let pages = rv
        .rev()
        .map_err(|e| Error::new(minijinja::ErrorKind::InvalidOperation, e.to_string()))?
        .pages
        .values()
        .filter(|p| p.is_article() && p.is_listed(&viewer))
        .filter(|p| p.tags.iter().any(|t| t == "rust"))
        .collect::<Vec<_>>();

    let page = (*pages.choose(&mut rand::thread_rng()).ok_or_else(|| {
        Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "No articles available",
        )
    })?)
    .clone();

    Ok(page.to_val())
}

fn get_tag_listing(state: &minijinja::State, args: Kwargs) -> Result<Value, Error> {
    let tag = args.get::<String>("tag")?;
    let page_number = args.get::<usize>("page_number").unwrap_or(1);
    let per_page = args.get::<usize>("per_page").unwrap_or(25);

    let zero_indexed_page_number = page_number.checked_sub(1).ok_or_else(|| {
        Error::new(
            minijinja::ErrorKind::InvalidOperation,
            "page out of range: must be >= 1",
        )
    })?;
    let viewer = Viewer {
        is_admin: false,
        has_bronze: false,
        has_silver: false,
    };

    let rv = get_revision_view(state);
    let rev = rv.rev().mj()?;

    let paths = match rev.tags.get_or_help(ResourceKind::Tag, &tag) {
        Ok(paths) => paths,
        Err(_e) => {
            // return an empty listing
            return Ok(Value::from(Listing {
                kind: ListingKind::Articles,
                items: Default::default(),
                page_number,
                per_page,
                has_more: false,
            }));
        }
    };
    let mut pages = paths
        .iter()
        .filter_map(|p| rev.pages.get(p))
        .filter(|p| p.is_listed(&viewer))
        .sorted_by_key(|p| std::cmp::Reverse(p.date))
        .skip(zero_indexed_page_number * per_page)
        .take(per_page + 1)
        .cloned()
        .map(LoadedPageVal)
        .collect::<Vec<_>>();
    let has_more = pages.len() > per_page;
    if has_more {
        pages.pop();
    }
    Ok(Value::from(Listing {
        kind: ListingKind::Articles,
        items: pages,
        page_number,
        per_page,
        has_more,
    }))
}

fn search_page(state: &minijinja::State, args: Kwargs) -> Result<Value, Error> {
    let query = args.get::<String>("query")?;
    let per_page = args.get::<usize>("per_page")?;
    let page_number = args.get::<usize>("page_number")?;
    args.assert_all_used()?;

    let viewer = Viewer {
        is_admin: false,
        has_bronze: false,
        has_silver: false,
    };

    let rv = get_revision_view(state);
    let gv = state
        .lookup("globals")
        .and_then(|v| v.downcast_object::<GlobalsVal>())
        .ok_or_else(|| Error::new(minijinja::ErrorKind::InvalidOperation, "globals not found"))?;

    let results = gv
        .index
        .search(rv.as_ref(), &viewer, &query, per_page, page_number);
    Ok(SearchResultsVal(results).into())
}

pub(crate) fn register_all(environment: &mut Environment<'static>) {
    ///////////////////////////////////////////////////////////////
    // functions
    ///////////////////////////////////////////////////////////////

    environment.add_function("asset_url", asset_url);
    environment.add_function("get_media", get_media);
    environment.add_function("get_recent_pages", get_recent_pages);
    environment.add_function("url_encode", url_encode);
    environment.add_function("html_escape", html_escape);
    environment.add_function("get_page_from_route", get_page_from_route);
    environment.add_function("get_page_from_path", get_page_from_path);

    environment.add_function("all_icons", all_icons);
    environment.add_function("basic_markdown", basic_markdown);

    environment.add_function("random_article", random_article);
    environment.add_function("get_tag_listing", get_tag_listing);
    environment.add_function("search_page", search_page);

    ///////////////////////////////////////////////////////////////
    // filters
    ///////////////////////////////////////////////////////////////

    environment.add_filter("asset_url", asset_url);
    environment.add_filter("url_encode", url_encode);
    environment.add_filter("html_escape", html_escape);
    environment.add_filter("truncate_html", truncate_html);
    environment.add_filter("truncate", truncate);
    environment.add_filter("downcase", downcase);
    environment.add_filter("shuffle", shuffle);
    environment.add_filter("urlencode", urlencode);
    environment.add_filter("fragment_urlencode", fragment_urlencode);
    environment.add_filter("to_json", to_json);
    environment.add_filter("basic_markdown", basic_markdown);
    environment.add_filter("escape_for_attribute", escape_for_attribute);

    environment.add_filter("format_time_ago", format_time_ago);
    environment.add_filter("format_rfc3339", format_rfc3339);
    environment.add_filter("format_month_year", format_month_year);
    environment.add_filter("format_day_month_year", format_day_month_year);
    environment.add_filter("is_future", is_future);

    environment.set_unknown_method_callback(unknown_method_callback);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_urlencode_vs_fragment_urlencode() {
        let input = " unsafe code. There’s no such thing as “two Rusts”,".to_string();

        let urlencoded = urlencode(input.clone());
        let fragment_urlencoded = fragment_urlencode(input.clone());

        // Standard urlencoding (like x-www-form-urlencoded) encodes spaces, quotes, comma, etc.
        // but typically leaves '.' unencoded.
        let expected_urlencode = "%20unsafe%20code.%20There%E2%80%99s%20no%20such%20thing%20as%20%E2%80%9Ctwo%20Rusts%E2%80%9D%2C";
        assert_eq!(urlencoded, expected_urlencode);

        // Fragment urlencoding encodes fewer characters by default, but our custom set adds '.'
        // Note: It also percent-encodes characters like `’`, `“`, `”` because they are outside the ASCII range allowed by the spec.
        let expected_fragment_urlencode = "%20unsafe%20code%2E%20There%E2%80%99s%20no%20such%20thing%20as%20%E2%80%9Ctwo%20Rusts%E2%80%9D%2C";
        assert_eq!(fragment_urlencoded, expected_fragment_urlencode);

        // The key difference in this case is the encoding of '.'
        assert_ne!(urlencoded, fragment_urlencoded);
    }

    #[test]
    fn test_fragment_urlencode_handles_dash() {
        let input = "section-1.2-heading".to_string();
        let fragment_urlencoded = fragment_urlencode(input);
        // '-' should be encoded as %2D, '.' as %2E
        assert_eq!(fragment_urlencoded, "section%2D1%2E2%2Dheading");

        // Standard urlencode would not encode '-' or '.'
        let urlencoded = urlencode("section-1.2-heading".to_string());
        assert_eq!(urlencoded, "section-1.2-heading");
    }

    #[test]
    fn test_fragment_urlencode_other_chars() {
        // Characters included in FRAGMENT_ENCODE_SET_BASE
        let input = "a b\"c<d>e`f".to_string();
        let fragment_urlencoded = fragment_urlencode(input);
        assert_eq!(fragment_urlencoded, "a%20b%22c%3Cd%3Ee%60f");

        // Standard urlencode encodes space, " but not < > `
        let urlencoded = urlencode("a b\"c<d>e`f".to_string());
        assert_eq!(urlencoded, "a%20b%22c%3Cd%3Ee%60f"); // urlencoding::encode does encode these
    }
}
