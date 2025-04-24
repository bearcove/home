use autotrait::autotrait;

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
    /// Truncate HTML to a given number of paragraphs
    ///
    /// @arg max: The number of characters to keep - this *will* go over that
    /// because it operates on paragraphs
    fn truncate_html(&self, input: &str, max: u64) -> String {
        use lol_html::{HtmlRewriter, Settings, element, html_content::TextType, text};

        let mut output: Vec<u8> = Vec::new();
        let output_sink = |c: &[u8]| {
            output.extend_from_slice(c);
        };

        let char_count = std::cell::RefCell::new(0u64);
        let mut skip: bool = false;

        fn keep_even_when_skipping(tag_name: &str) -> bool {
            #![allow(clippy::match_like_matches_macro)]

            match tag_name {
                "span" | "em" | "a" | "b" | "i" | "br" | "strong" | "small" | "sub" | "sup"
                | "mark" | "u" | "code" | "q" | "cite" | "dfn" | "abbr" | "time" | "var"
                | "kbd" | "samp" => true,
                _ => false,
            }
        }

        // TODO: can we do an early return here?
        let mut rewriter = HtmlRewriter::new(
            Settings {
                element_content_handlers: vec![
                    element!("*", |el| {
                        if *char_count.borrow() > max {
                            skip = true;
                        }

                        if skip {
                            if keep_even_when_skipping(&el.tag_name()) {
                                // are we sure? maybe it's a code-block?
                                if el
                                    .get_attribute("class")
                                    .unwrap_or_default()
                                    .contains("code-block")
                                {
                                    // definitely remove then
                                    el.remove();
                                }
                            } else {
                                el.remove();
                            }
                        }
                        Ok(())
                    }),
                    text!("*", |txt| {
                        if matches!(txt.text_type(), TextType::Data) {
                            *char_count.borrow_mut() += txt.as_str().len() as u64;
                        }

                        Ok(())
                    }),
                ],
                ..Settings::default()
            },
            output_sink,
        );

        let max_write_size: usize = 4096;
        let mut remaining = input.as_bytes();
        while !remaining.is_empty() {
            let chunk_size = remaining.len().min(max_write_size);
            rewriter.write(&remaining[..chunk_size]).unwrap();
            remaining = &remaining[chunk_size..];
        }
        String::from_utf8(output).unwrap()
    }
}
