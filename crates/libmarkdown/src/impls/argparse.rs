use eyre::bail;
use template_types::{DataObject, DataValue};

pub(crate) fn parse_emphasis_shortcode(input: &str) -> eyre::Result<(String, DataObject)> {
    macro_rules! trace {
        ($($msg:tt)*) => {
            // eprintln!($($msg)*);
        };
    }

    let mut parts = input.splitn(2, '(');
    let name = parts.next().unwrap().trim().to_string();
    let args_str = if let Some(part) = parts.next() {
        match part.trim().strip_suffix(')') {
            Some(stripped) => stripped,
            None => bail!("expected closing parenthesis for arguments: {input}"),
        }
    } else {
        ""
    };

    let mut args = DataObject::new();

    #[derive(Debug, Clone, Copy)]
    struct Span {
        start: usize,
        length: usize,
    }

    impl Span {
        fn extend_till(&mut self, offset: usize, c: char) {
            trace!(
                "offset={}, self.start={}, c='{}', c.len_utf8={}, self.length={}",
                offset,
                self.start,
                c,
                c.len_utf8(),
                self.length
            );
            self.length = offset - self.start + c.len_utf8();
        }
    }

    #[derive(Debug)]
    enum State<'a> {
        Key(Span),
        Value { key: &'a str, value: Value },
        AfterValue,
    }

    #[derive(Debug)]
    enum Value {
        String { content: CowSpan, escape: bool },
        Boolean { content: Span },
        Number { content: Span },
        Unknown,
    }

    /// A CowStr that can either be a Span or a String
    #[derive(Debug)]
    enum CowSpan {
        Span(Span),
        String(String),
    }

    let mut state = State::Key(Span {
        start: 0,
        length: 0,
    });

    for (i, c) in args_str
        .char_indices()
        .chain(std::iter::once((usize::MAX, '💣')))
    {
        'process_char: loop {
            // trace!("c={c:?}, state={state:?}");

            match &mut state {
                State::AfterValue => match c {
                    '💣' => {
                        // EOF, we're done
                    }
                    ',' => {
                        // ready to read the next key
                        state = State::Key(Span {
                            start: i + c.len_utf8(),
                            length: 0,
                        });
                    }
                    c => {
                        if c.is_whitespace() {
                            // cool, just ignore
                        } else {
                            bail!("unexpected character after value: {c}. full string: {args_str}");
                        }
                    }
                },
                State::Key(key) => {
                    match c {
                        '💣' => {
                            let key_str = &args_str[key.start..][..key.length].trim();
                            if key_str.is_empty() {
                                // okay I guess there's no args
                            } else {
                                bail!(
                                    "unexpected EOF while reading shortcode key. full string: {args_str}"
                                );
                            }
                        }
                        // If we encounter an '=', it means we've finished reading the key
                        // and are starting to read the value.
                        '=' => {
                            let key_str = &args_str[key.start..][..key.length].trim();
                            if key_str.is_empty() {
                                bail!("empty key in shortcode arguments: {args_str}");
                            }

                            state = State::Value {
                                key: key_str,
                                value: Value::Unknown,
                            };
                        }
                        // For any other character, we increment the length of the key.
                        _ => {
                            key.extend_till(i, c);
                        }
                    }
                }
                State::Value { key, value } => {
                    match value {
                        Value::Unknown => {
                            *value = match c {
                                '💣' => {
                                    bail!(
                                        "unexpected EOF: expecting value. full string: {args_str}"
                                    );
                                }
                                // true or false?
                                't' | 'f' => Value::Boolean {
                                    content: Span {
                                        start: i,
                                        length: 1,
                                    },
                                },
                                // number?
                                '0'..='9' => Value::Number {
                                    content: Span {
                                        start: i,
                                        length: 1,
                                    },
                                },
                                // string?
                                '"' | '“' => Value::String {
                                    content: CowSpan::Span(Span {
                                        start: i + c.len_utf8(),
                                        length: 0,
                                    }),
                                    escape: false,
                                },
                                '‘' | '’' => {
                                    bail!(
                                        "smart quotes are not legal: {c}. full string: {args_str}"
                                    );
                                }
                                _ => {
                                    if c.is_whitespace() {
                                        // ignore, wait for the value to start
                                        Value::Unknown
                                    } else {
                                        bail!(
                                            "unexpected character in value: {c}. full string: {args_str}"
                                        );
                                    }
                                }
                            }
                        }
                        Value::String { content, escape } => {
                            if *escape {
                                *escape = false;
                                if let CowSpan::Span(span) = content {
                                    *content = CowSpan::String(
                                        args_str[span.start..][..span.length].to_string(),
                                    );
                                }
                                let s = match content {
                                    CowSpan::Span(_) => unreachable!(),
                                    CowSpan::String(s) => s,
                                };

                                match c {
                                    '💣' => {
                                        bail!(
                                            "unexpected EOF: expecting escape. full string: {args_str}"
                                        );
                                    }
                                    '\\' => {
                                        s.push('\\');
                                    }
                                    '\n' => {
                                        s.push('\n');
                                    }
                                    '"' => {
                                        s.push('"');
                                    }
                                    'n' => {
                                        s.push('\n');
                                    }
                                    't' => {
                                        s.push('\t');
                                    }
                                    'r' => {
                                        s.push('\r');
                                    }
                                    other => {
                                        bail!("bad escape: {other}. full string: {args_str}");
                                    }
                                }
                            } else {
                                match c {
                                    '💣' => {
                                        bail!(
                                            "unexpected EOF: unclosed string. full string: {args_str}"
                                        );
                                    }
                                    '\\' => *escape = true,
                                    '"' | '”' => {
                                        match content {
                                            CowSpan::Span(s) => {
                                                let borrowed_s = &args_str[s.start..][..s.length];
                                                args.insert(
                                                    key.to_owned(),
                                                    DataValue::String(borrowed_s.to_owned()),
                                                );
                                            }
                                            CowSpan::String(s) => {
                                                let s = std::mem::take(s);
                                                args.insert(key.to_owned(), DataValue::String(s));
                                            }
                                        }
                                        state = State::AfterValue;
                                    }
                                    _ => match content {
                                        CowSpan::Span(span) => {
                                            trace!(
                                                "ended up in .... Value::String, a non-closing char, span is {span:?}, i={i:?}, c={c:?}"
                                            );
                                            span.extend_till(i, c);
                                        }
                                        CowSpan::String(s) => {
                                            s.push(c);
                                        }
                                    },
                                }
                            }
                        }
                        Value::Number { content } => match c {
                            '0'..='9' => {
                                content.extend_till(i, c);
                            }
                            _ => {
                                // parse the number and add it to the map
                                let num_str = &args_str[content.start..][..content.length].trim();
                                let num = match num_str.parse::<i32>() {
                                    Ok(n) => n,
                                    Err(_) => bail!("couldn't parse number: {num_str}"),
                                };
                                args.insert(key.to_owned(), DataValue::Number(num));
                                state = State::AfterValue;

                                // we didn't consume the character, so we need to continue
                                continue 'process_char;
                            }
                        },
                        Value::Boolean { content } => {
                            if c == '💣' {
                                bail!(
                                    "unexpected EOF: expecting boolean value. full string: {args_str}"
                                );
                            }
                            content.extend_till(i, c);

                            let boolean_str = args_str[content.start..][..content.length].trim();
                            match boolean_str {
                                "true" => {
                                    args.insert(key.to_owned(), DataValue::Boolean(true));
                                    state = State::AfterValue;
                                }
                                "false" => {
                                    args.insert(key.to_owned(), DataValue::Boolean(false));
                                    state = State::AfterValue;
                                }
                                _ => {
                                    // if the string is already 5 characters long, it's not a valid
                                    // boolean, so we bail
                                    if boolean_str.len() >= 5 {
                                        bail!(
                                            "invalid boolean value: {boolean_str}. full string: {args_str}"
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            break 'process_char;
        }
    }

    Ok((name, args))
}

#[cfg(test)]
mod tests {
    use template_types::DataValue;

    use super::*;

    #[test]
    fn parse_shortcode_args_valid() {
        let input = r#"shortcode(key1="value1", key2=42, key3=true, key4=false)"#;
        let (name, args) = parse_emphasis_shortcode(input).unwrap();
        assert_eq!(name, "shortcode");
        assert_eq!(
            args.get("key1"),
            Some(&DataValue::String("value1".to_owned()))
        );
        assert_eq!(args.get("key2"), Some(&DataValue::Number(42)));
        assert_eq!(args.get("key3"), Some(&DataValue::Boolean(true)));
        assert_eq!(args.get("key4"), Some(&DataValue::Boolean(false)));
    }

    #[test]
    fn parse_shortcode_args_invalid_syntax() {
        let inputs = vec![
            r#"shortcode(key1="value1", key2=42, key3=true, key4)"#, // Missing value
            r#"shortcode(key1="value1", key2=42, key3=true, key4=)"#, // Missing value after '='
            r#"shortcode(key1="value1", key2=42, key3=true, =false)"#, // Missing key
            r#"shortcode(key1="value1", key2=42, key3=true, key4=false"#, // Missing closing parenthesis
        ];

        for input in inputs {
            assert!(parse_emphasis_shortcode(input).is_err());
        }
    }

    #[test]
    fn parse_shortcode_args_string_escapes() {
        let input = r#"shortcode(key1="value with \"escaped quotes\" and \\backslashes\\")"#;
        let (name, args) = parse_emphasis_shortcode(input).unwrap();
        assert_eq!(name, "shortcode");
        assert_eq!(
            args.get("key1"),
            Some(&DataValue::String(
                r#"value with "escaped quotes" and \backslashes\"#.into()
            ))
        );
    }

    #[test]
    fn parse_shortcode_args_empty_string() {
        let input = r#"shortcode(key1="")"#;
        let (name, args) = parse_emphasis_shortcode(input).unwrap();
        assert_eq!(name, "shortcode");
        assert_eq!(args.get("key1"), Some(&DataValue::String("".to_owned())));
    }

    #[test]
    fn parse_shortcode_args_no_args() {
        let input = r#"shortcode()"#;
        let (name, args) = parse_emphasis_shortcode(input).unwrap();
        assert_eq!(name, "shortcode");
        assert!(args.is_empty());
    }

    #[test]
    fn parse_shortcode_args_number_with_whitespace() {
        let input = r#"shortcode(key1=  42  )"#;
        let (name, args) = parse_emphasis_shortcode(input).unwrap();
        assert_eq!(name, "shortcode");
        assert_eq!(args.get("key1"), Some(&DataValue::Number(42)));
    }

    #[test]
    fn parse_shortcode_args_smart_quotes() {
        let input = r#"shortcode(key1=“value”)"#;
        let (name, args) = parse_emphasis_shortcode(input).unwrap();
        assert_eq!(name, "shortcode");
        assert_eq!(
            args.get("key1"),
            Some(&DataValue::String("value".to_owned()))
        );
    }
}
