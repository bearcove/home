use cssparser::{Parser, ParserInput, Token};
use std::path::Path;

fn main() -> eyre::Result<()> {
    let out_dir = std::env::var("OUT_DIR").expect("OUT_DIR not set");
    let output_path = Path::new(&out_dir).join("icons.rs");

    let css_path = "src/icons.css";
    println!("cargo:rerun-if-changed={css_path}");
    let css_content = fs_err::read_to_string(css_path).expect("Failed to read CSS file");

    let mut input = ParserInput::new(&css_content);
    let mut parser = Parser::new(&mut input);

    let mut output = String::new();
    output.push_str("// Auto-generated file\n\n");

    while let Ok(token) = parser.next() {
        if let Token::Ident(ident) = token {
            let class_name = ident.to_string();
            let constant_name = class_name.replace("-", "_").to_uppercase();
            eprintln!(
                "Class name: {class_name}, Constant name: {constant_name}"
            );

            if let Token::Colon = parser.next().unwrap() {
                if let Token::Ident(ident) = parser.next().unwrap() {
                    if *ident == "before" {
                        if let Token::CurlyBracketBlock = parser.next().unwrap() {
                            eprintln!("Found curly bracket block");
                            let content_prop = parser
                                .parse_nested_block(|parser| loop {
                                    match parser.next()? {
                                        Token::Ident(ident) if *ident == "content" => {
                                            if let Ok(Token::Colon) = parser.next() {
                                                if let Ok(Token::QuotedString(s)) = parser.next() {
                                                    let content_prop: String = s.as_ref().into();
                                                    return Ok::<_, cssparser::ParseError<'_, ()>>(
                                                        content_prop,
                                                    );
                                                }
                                            }
                                        }
                                        other => {
                                            eprintln!("Ignoring other: {other:?}");
                                        }
                                    }
                                })
                                .unwrap();
                            use std::fmt::Write;

                            writeln!(
                                &mut output,
                                "pub const {constant_name}: &str = {content_prop:?};"
                            )
                            .unwrap();
                        }
                    }
                }
            }
        }
    }

    fs_err::write(&output_path, output).expect("Failed to write to icons.rs");

    Ok(())
}
