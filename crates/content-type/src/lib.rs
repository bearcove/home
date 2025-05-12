use facet::Facet;

macro_rules! content_types {
    ($($variant:ident => { ext: $ext:literal, mime: $mime:literal, serial: $serial:literal }),* $(,)?) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
        #[repr(u8)]
        pub enum ContentType {
            $($variant),*
        }

        impl ContentType {
            pub const fn as_str(&self) -> &'static str {
                match self {
                    $(ContentType::$variant => $mime),*
                }
            }

            pub const fn ext(&self) -> &'static str {
                match self {
                    $(ContentType::$variant => $ext),*
                }
            }
        }

        impl std::fmt::Display for ContentType {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.as_str())
            }
        }

        merde::derive! {
            impl (Serialize, Deserialize) for
            enum ContentType string_like {
                $($serial => $variant),*
            }
        }

        impl ContentType {
            pub fn guess_from_path(path: &str) -> Option<Self> {
                let guess = match path.rfind('.') {
                    Some(dot) => path[dot + 1..].to_ascii_lowercase(),
                    None => String::new(),
                };
                match guess.as_str() {
                    $($ext => Some(Self::$variant),)*
                    _ => None,
                }
            }
        }
    };
}

content_types! {
    Atom => { ext: "xml", mime: "application/atom+xml; charset=utf-8", serial: "atom" },
    HTML => { ext: "html", mime: "text/html; charset=utf-8", serial: "html" },
    CSS => { ext: "css", mime: "text/css; charset=utf-8", serial: "css" },
    SCSS => { ext: "scss", mime: "text/x-scss; charset=utf-8", serial: "scss" },
    JSON => { ext: "json", mime: "application/json; charset=utf-8", serial: "json" },
    JXL => { ext: "jxl", mime: "image/jxl", serial: "jxl" },
    HEIC => { ext: "heic", mime: "image/heic", serial: "heic" },
    AVIF => { ext: "avif", mime: "image/avif", serial: "avif" },
    WEBP => { ext: "webp", mime: "image/webp", serial: "webp" },
    PNG => { ext: "png", mime: "image/png", serial: "png" },
    JPG => { ext: "jpg", mime: "image/jpeg", serial: "jpg" },
    GIF => { ext: "gif", mime: "image/gif", serial: "gif" },
    SVG => { ext: "svg", mime: "image/svg+xml", serial: "svg" },
    ICO => { ext: "ico", mime: "image/x-icon", serial: "ico" },
    MP4 => { ext: "mp4", mime: "video/mp4", serial: "mp4" },
    WebM => { ext: "webm", mime: "video/webm", serial: "webm" },
    M4A => { ext: "m4a", mime: "audio/mp4", serial: "m4a" },
    OGG => { ext: "ogg", mime: "audio/ogg", serial: "ogg" },
    MP3 => { ext: "mp3", mime: "audio/mpeg", serial: "mp3" },
    FLAC => { ext: "flac", mime: "audio/flac", serial: "flac" },
    WOFF2 => { ext: "woff2", mime: "font/woff2", serial: "woff2" },
    Js => { ext: "js", mime: "application/javascript;charset=utf-8", serial: "js" },
    JsSourcemap => { ext: "js.map", mime: "application/json", serial: "js.map" },
    WASM => { ext: "wasm", mime: "application/wasm", serial: "wasm" },
    AAC => { ext: "aac", mime: "audio/aac", serial: "aac" },
    Markdown => { ext: "md", mime: "text/markdown; charset=utf-8", serial: "markdown" },
    DrawIO => { ext: "drawio", mime: "application/drawio", serial: "drawio" },
    OctetStream => { ext: "bin", mime: "application/octet-stream", serial: "octet-stream" },
    Jinja => { ext: "jinja", mime: "application/x-jinja", serial: "jinja" }
}
