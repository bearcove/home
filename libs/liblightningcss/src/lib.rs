include!(".dylo/spec.rs");
include!(".dylo/support.rs");

#[cfg(feature = "impl")]
#[derive(Default)]
struct ModImpl;

#[dylo::export]
impl Mod for ModImpl {
    fn minify(&self, unminified: &str) -> Result<String, String> {
        use lightningcss::{
            printer::PrinterOptions,
            stylesheet::{MinifyOptions, ParserOptions, StyleSheet},
            targets::Browsers,
        };

        let mut stylesheet = StyleSheet::parse(unminified, ParserOptions::default())
            .map_err(|e| format!("error parsing CSS: {e}"))?;

        let mut min_options = MinifyOptions::default();
        min_options.targets.browsers = Browsers::from_browserslist(["cover 99.5%"])
            .map_err(|e| format!("error parsing browserslist: {e}"))?;
        stylesheet
            .minify(min_options)
            .map_err(|e| format!("error minifying CSS: {e}"))?;

        let print_opts = PrinterOptions {
            minify: true,
            ..Default::default()
        };
        let css = stylesheet
            .to_css(print_opts)
            .map_err(|e| format!("error printing CSS: {e}"))?;

        Ok(css.code)
    }
}
