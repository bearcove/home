use core::fmt;

use conflux::InputPath;
use eyre::bail;

pub trait PrettifyExt<T> {
    fn prettify_minijinja_error(self) -> eyre::Result<T>;
}

impl<T, E> PrettifyExt<T> for Result<T, E>
where
    E: std::error::Error + 'static,
{
    fn prettify_minijinja_error(self) -> eyre::Result<T> {
        match self {
            Ok(val) => Ok(val),
            Err(e) => {
                let mut buf: String = Default::default();
                use std::fmt::Write;

                let mut e: &dyn std::error::Error = &e;
                for i in 1.. {
                    writeln!(&mut buf, "{i}. {e}").unwrap();
                    if let Some(minijinja_err) = e.downcast_ref::<minijinja::Error>() {
                        if let Some(detail) = minijinja_err.detail() {
                            write!(&mut buf, "\ndetail: {}", detail).unwrap();
                        } else {
                            write!(&mut buf, "(no detail)").unwrap();
                        }
                        write!(&mut buf, "{}", prettify_minijinja_error(minijinja_err)).unwrap();
                    }

                    match e.source() {
                        Some(source) => {
                            e = source;
                        }
                        None => break,
                    }
                }
                bail!("{buf}")
            }
        }
    }
}

pub(crate) fn prettify_minijinja_error(e: &minijinja::Error) -> String {
    let debug_info_source: String = format!("{}", e.display_debug_info());

    if matches!(
        std::env::var("DEBUG_MINIJINJA_ERRORS").as_deref(),
        Ok("1") | Ok("true")
    ) {
        eprintln!("original minijinja error:\n\n{e}\n\n");
    }

    if debug_info_source.trim().is_empty() {
        return "\n".into();
    }

    // here's what the debug info looks like:
    //
    // --------------------------------- index.html ----------------------------------
    //    1 > {% include "html/prologue.html" %}
    //      i    ^^^^^^^^^^^^^^^^^^^^^^^^^^^^ could not render include
    //    2 |
    //    3 | {% set articles = root.get_articles(limit=8) %}
    //    4 | {% set series = root.get_series(limit=4) %}
    // ~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~~
    // No referenced variables
    // -------------------------------------------------------------------------------
    //
    // we want to split the column number and the source file, so that we can
    // highlight them in different colors.
    //
    // let's collect source lines, keep track of 'i' lines (informational lines) separately,
    // so that we can highlight the entire source code, then _reformat_ the whole thing with
    // syntax colors AND colors on the diagnostics.
    //
    // we're going to have to write a little parser for their output format, and fill this data
    // structure:
    let mut start_line_number: Option<usize> = None;
    let mut file_name: Option<String> = None;
    let mut source_lines: Vec<String> = Default::default();
    let mut other_lines: Vec<String> = Default::default();

    struct DiagLine {
        source_line_idx: usize,
        diag_text: String,
    }

    impl fmt::Display for DiagLine {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            writeln!(f, "\x1b[34m{}\x1b[0m", self.diag_text)
        }
    }

    let mut diag_lines: Vec<DiagLine> = Default::default();

    #[derive(Debug)]
    enum State {
        ExpectFileName,
        ReadingSourceLines,
        ReadingOtherLines,
        Done,
    }
    let mut state = State::ExpectFileName;

    for line in debug_info_source.lines() {
        match state {
            State::ExpectFileName => {
                if line.starts_with("---") {
                    file_name = Some(line.trim_matches('-').trim().to_string());
                    state = State::ReadingSourceLines;
                }
            }
            State::ReadingSourceLines => {
                if line.starts_with("~~~") {
                    state = State::ReadingOtherLines;
                } else if !line.is_empty() {
                    if let Some(rest) = line.trim().strip_prefix("i") {
                        diag_lines.push(DiagLine {
                            source_line_idx: source_lines.len() - 1,
                            diag_text: rest.to_string(),
                        });
                    } else {
                        let line = line.trim();

                        let mut chars = line.chars();

                        let number_chars: String =
                            chars.by_ref().take_while(|c| c.is_ascii_digit()).collect();
                        let line_number = number_chars.parse::<usize>().unwrap();

                        let mut chars = chars.by_ref().skip_while(|c| c.is_whitespace());

                        let delimiter = chars.next().unwrap();
                        if !">|".contains(delimiter) {
                            panic!("unexpected delimiter: {delimiter}");
                        }

                        // skip ONE space
                        chars.next();

                        let content = chars.collect::<String>();

                        if start_line_number.is_none() {
                            start_line_number = Some(line_number);
                        }
                        source_lines.push(content);
                    }
                }
            }
            State::ReadingOtherLines => {
                if line.starts_with("---") {
                    state = State::Done;
                } else if line != "No referenced variables" {
                    other_lines.push(line.to_string());
                }
            }
            State::Done => {
                eprintln!("ignoring unexpected line: {line}");
                break;
            }
        }
    }

    use std::fmt::Write;

    // Now we have all the parsed information, we can format it nicely
    let mut result = String::from("\n");

    let file_name = file_name.unwrap_or_else(|| "<unknown>".to_string());
    let input_path = InputPath::from(format!("/templates/{file_name}.jinja"));
    let file_url = format!("home://{input_path}",);
    writeln!(result, "============= [{input_path}]({file_url})").unwrap();

    let start_line_number = start_line_number.unwrap_or(1);
    for (idx, line) in source_lines.iter().enumerate() {
        let line_number = start_line_number + idx;
        let line = line
            .replace("{{", "\x1b[32m{{\x1b[0m")
            .replace("}}", "\x1b[32m}}\x1b[0m")
            .replace("{%", "\x1b[33m{%\x1b[0m")
            .replace("%}", "\x1b[33m%}\x1b[0m")
            .replace("{#", "\x1b[36m{#\x1b[0m")
            .replace("#}", "\x1b[36m#}\x1b[0m");

        let line_link = format!("[{:4}]({file_url}:{line_number})", line_number);

        if let Some(diag) = diag_lines.iter().find(|d| d.source_line_idx == idx) {
            writeln!(result, "\x1b[90m{} >\x1b[0m {}", line_link, line).unwrap();
            let last_caret_index = diag.diag_text.rfind('^').unwrap_or(diag.diag_text.len());
            let formatted_diag = format!(
                "{}\x1b[31m{}",
                diag.diag_text[..=last_caret_index].replace("^", "\x1b[33m^\x1b[0m"),
                &diag.diag_text[last_caret_index + 1..]
            );
            writeln!(result, "\x1b[90m     i\x1b[0m {}", formatted_diag).unwrap();
        } else {
            writeln!(result, "\x1b[90m{} |\x1b[0m {}", line_link, line).unwrap();
        }
    }

    if other_lines.is_empty() {
        writeln!(result, "\x1b[90m{:─<80}\x1b[0m", "").unwrap();
    } else {
        writeln!(result, "\x1b[90m{:═<80}\x1b[0m", "").unwrap();
        for line in other_lines {
            writeln!(result, "{}", line).unwrap();
        }
        writeln!(result, "\x1b[90m{:─<80}\x1b[0m", "").unwrap();
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::Environment;

    #[test]
    fn test_prettify_minijinja_error() {
        let source = r#"
            <meta property="twitter:image" content="{{ opengraph_image }}">

            {% if page and page.plain_text %}
            {% set og_description = page.plain_text | truncate: 180 %}
            {% else %}
            {% set og_description = "amos loves to tinker" %}
            {% endif %}
        "#;
        let environment = Environment::new();
        let err = environment
            .render_named_str("something.jinja", source, ())
            .unwrap_err();

        let prettified = prettify_minijinja_error(&err);
        insta::assert_snapshot!(prettified);
    }
}
