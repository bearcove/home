use config_types::{FontStyle, FontWeight};
use lightningcss::{
    printer::PrinterOptions,
    properties::{
        Property,
        font::{self, FontFamily},
    },
    stylesheet::{ParserOptions, StyleSheet},
    traits::ToCss,
};
use quick_xml::events::Event;
use std::collections::{HashMap, HashSet};
use std::fmt;

#[derive(Default, Clone, Debug, PartialEq, Eq, Hash)]
pub struct Typo {
    pub family: Option<String>,
    pub weight: Option<FontWeight>,
    pub style: Option<FontStyle>,
}

impl fmt::Display for Typo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Typo {{ ")?;
        if let Some(ref family) = self.family {
            write!(f, "family: {:?}, ", family)?;
        } else {
            write!(f, "family: \"unknown\", ")?;
        }

        if let Some(weight) = self.weight {
            write!(f, "weight: {}, ", weight)?;
        } else {
            write!(f, "weight: \"unknown\", ")?;
        }

        if let Some(style) = self.style {
            write!(f, "style: {}", style)?;
        } else {
            write!(f, "style: \"unknown\" }}")?;
        }

        Ok(())
    }
}

#[derive(Debug)]
enum StackItem {
    StyleElement { markup: String },
    SwitchElement,
    TypoOverride(Typo),
    OtherElement,
}

pub type ClassStyles = HashMap<String, Typo>;

struct CharUsageDetector {
    stack: Vec<StackItem>,
    chars_per_typo: HashMap<Typo, HashSet<char>>,
    class_styles: ClassStyles,
}

impl fmt::Display for CharUsageDetector {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "\x1b[32mInjectorState {{")?;
        writeln!(f, "  stack: [")?;
        for item in &self.stack {
            match item {
                StackItem::StyleElement { markup } => {
                    writeln!(
                        f,
                        "    \x1b[34mStyleElement {{ markup: {:?} }}\x1b[0m",
                        markup
                    )?;
                }
                StackItem::SwitchElement => {
                    writeln!(f, "    \x1b[35mSwitchElement\x1b[0m")?;
                }
                StackItem::TypoOverride(typo) => {
                    writeln!(f, "    \x1b[36m{}\x1b[0m", typo)?;
                }
                StackItem::OtherElement => {
                    writeln!(f, "    \x1b[33mOtherElement\x1b[0m")?;
                }
            }
        }
        writeln!(f, "  ],")?;
        writeln!(f, "  chars_per_typo: {{")?;
        for (typo, chars) in &self.chars_per_typo {
            writeln!(f, "    {}: {:?},", typo, chars)?;
        }
        writeln!(f, "  }}")?;
        writeln!(f, "}}\x1b[0m")?;
        Ok(())
    }
}

impl CharUsageDetector {
    fn new() -> Self {
        Self {
            stack: Vec::new(),
            chars_per_typo: Default::default(),
            class_styles: Default::default(),
        }
    }

    fn typo(&self) -> Typo {
        let mut typo = Typo::default();

        for item in self.stack.iter().rev() {
            if let StackItem::TypoOverride(s) = item {
                if typo.family.is_none() && s.family.is_some() {
                    typo.family = s.family.clone();
                }
                if typo.weight.is_none() && s.weight.is_some() {
                    typo.weight = s.weight;
                }
                if typo.style.is_none() && s.style.is_some() {
                    typo.style = s.style;
                }
            }
        }

        if typo.weight.is_none() {
            typo.weight = Some(FontWeight(400));
        }
        if typo.style.is_none() {
            typo.style = Some(FontStyle::Normal);
        }

        typo
    }

    fn record_used_chars(&mut self, s: &str) {
        if self
            .stack
            .iter()
            .any(|item| matches!(item, StackItem::SwitchElement))
        {
            return;
        }

        let typo = self.typo();
        for c in s.chars() {
            self.chars_per_typo
                .entry(typo.clone())
                .or_default()
                .insert(c);
        }
    }
}

/// Analyzes an SVG file to determine which characters are used with which typography settings
pub fn analyze_char_usage(input: &[u8]) -> eyre::Result<HashMap<Typo, HashSet<char>>> {
    let mut state = CharUsageDetector::new();
    let mut reader = quick_xml::Reader::from_reader(input);

    loop {
        match reader.read_event() {
            Ok(Event::Text(t)) => {
                let t = t.unescape()?;
                if let Some(StackItem::StyleElement { markup }) = state.stack.last_mut() {
                    markup.push_str(&t);
                } else {
                    state.record_used_chars(&t);
                }
            }
            Ok(Event::Start(e)) => match e.local_name().as_ref() {
                b"switch" => {
                    state.stack.push(StackItem::SwitchElement);
                }
                b"style" => {
                    state.stack.push(StackItem::StyleElement {
                        markup: String::new(),
                    });
                }
                b"b" => {
                    state.stack.push(StackItem::TypoOverride(Typo {
                        weight: Some(FontWeight(700)),
                        ..Default::default()
                    }));
                }
                b"i" => {
                    state.stack.push(StackItem::TypoOverride(Typo {
                        style: Some(FontStyle::Italic),
                        ..Default::default()
                    }));
                }
                _other => {
                    let mut class_name = None;
                    for attr in e.attributes() {
                        let attr = attr?;
                        if attr.key.local_name().as_ref() == b"class" {
                            class_name = Some(String::from_utf8_lossy(&attr.value).to_string());
                            break;
                        }
                    }

                    if let Some(class) = class_name {
                        if let Some(typo) = state.class_styles.get(&class) {
                            state.stack.push(StackItem::TypoOverride(typo.clone()));
                        } else {
                            state.stack.push(StackItem::OtherElement);
                        }
                    } else {
                        state.stack.push(StackItem::OtherElement);
                    }
                }
            },
            Ok(Event::End(_)) => {
                let popped = state.stack.pop();
                if let Some(StackItem::StyleElement { markup }) = popped {
                    // Parse the CSS using Lightning CSS
                    let parsed_css = StyleSheet::parse(&markup, ParserOptions::default())
                        .map_err(|e| eyre::eyre!("CSS parsing error: {e}"))?;

                    // Create a hash map to store class names and their corresponding font properties
                    let mut class_styles: ClassStyles = Default::default();

                    for rule in &parsed_css.rules.0 {
                        if let lightningcss::rules::CssRule::Style(style_rule) = rule {
                            for selector in &style_rule.selectors.0 {
                                if let Some(lightningcss::selector::Component::Class(class)) =
                                    selector.iter_raw_match_order().last()
                                {
                                    let class_name = class.0.to_string();
                                    let mut font_family: Option<String> = None;
                                    let mut font_weight: Option<FontWeight> = None;
                                    let mut font_style: Option<FontStyle> = None;

                                    for declaration in &style_rule.declarations.declarations {
                                        match declaration {
                                            Property::FontFamily(family) => {
                                                if let Some(FontFamily::FamilyName(family_name)) =
                                                    family.first()
                                                {
                                                    font_family =
                                                        Some(
                                                            family_name
                                                                .to_css_string(
                                                                    PrinterOptions::default(),
                                                                )
                                                                .map_err(|e| {
                                                                    eyre::eyre!(
                                                                        "CSS printing error: {e}"
                                                                    )
                                                                })?,
                                                        );
                                                }
                                            }
                                            Property::FontWeight(font::FontWeight::Absolute(
                                                weight,
                                            )) => match weight {
                                                font::AbsoluteFontWeight::Weight(w) => {
                                                    font_weight = Some(FontWeight(*w as u16));
                                                }
                                                font::AbsoluteFontWeight::Normal => {
                                                    font_weight = Some(FontWeight(400));
                                                }
                                                font::AbsoluteFontWeight::Bold => {
                                                    font_weight = Some(FontWeight(700));
                                                }
                                            },
                                            Property::FontStyle(style) => match style {
                                                font::FontStyle::Normal => {
                                                    font_style = Some(FontStyle::Normal);
                                                }
                                                font::FontStyle::Italic => {
                                                    font_style = Some(FontStyle::Italic);
                                                }
                                                font::FontStyle::Oblique(_) => {
                                                    font_style = Some(FontStyle::Italic);
                                                }
                                            },
                                            _ => {}
                                        }
                                    }

                                    class_styles.insert(
                                        class_name,
                                        Typo {
                                            family: font_family,
                                            weight: font_weight,
                                            style: font_style,
                                        },
                                    );
                                }
                            }
                        }
                    }

                    state.class_styles = class_styles;
                }
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(e) => return Err(e.into()),
        }
    }

    Ok(state.chars_per_typo)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_analyze_basic_usage() {
        let svg = include_str!("testdata/drawio-bold-test.svg");
        let result = analyze_char_usage(svg.as_bytes()).unwrap();

        // Check normal text with IosevkaFtl font
        let normal_typo = Typo {
            family: Some("IosevkaFtl".to_string()),
            weight: Some(FontWeight(400)),
            style: Some(FontStyle::Normal),
        };
        assert!(result.get(&normal_typo).unwrap().contains(&'a'));
        assert!(result.get(&normal_typo).unwrap().contains(&'b'));
        assert!(result.get(&normal_typo).unwrap().contains(&'c'));
        assert!(result.get(&normal_typo).unwrap().contains(&'d'));
        assert!(result.get(&normal_typo).unwrap().contains(&'i'));
        assert!(result.get(&normal_typo).unwrap().contains(&'j'));
        assert!(result.get(&normal_typo).unwrap().contains(&'k'));
        assert!(result.get(&normal_typo).unwrap().contains(&'l'));

        // Check bold text
        let bold_typo = Typo {
            family: Some("IosevkaFtl".to_string()),
            weight: Some(FontWeight(700)),
            style: Some(FontStyle::Normal),
        };
        assert!(result.get(&bold_typo).unwrap().contains(&'e'));
        assert!(result.get(&bold_typo).unwrap().contains(&'f'));
        assert!(result.get(&bold_typo).unwrap().contains(&'g'));
        assert!(result.get(&bold_typo).unwrap().contains(&'h'));
        assert!(result.get(&bold_typo).unwrap().contains(&'m'));
        assert!(result.get(&bold_typo).unwrap().contains(&'n'));
        assert!(result.get(&bold_typo).unwrap().contains(&'o'));
        assert!(result.get(&bold_typo).unwrap().contains(&'p'));
        assert!(result.get(&bold_typo).unwrap().contains(&'!'));

        // Previous italic assertion was incorrect - there is no italic text in the sample
        assert!(!result.contains_key(&Typo {
            family: Some("IosevkaFtl".to_string()),
            weight: Some(FontWeight(400)),
            style: Some(FontStyle::Italic),
        }));
    }
}
