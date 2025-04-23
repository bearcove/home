use autotrait::autotrait;

/// The result of a compression operation
pub struct CompressResult {
    pub content_encoding: Option<&'static str>,
    pub payload: bytes::Bytes,
}

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

#[derive(Debug)]
pub enum Error {
    Any(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Any(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for Error {}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Self::Any(err.to_string())
    }
}

#[autotrait]
impl Mod for ModImpl {
    fn compress(&self, input: bytes::Bytes, accept_encoding: &str) -> Result<CompressResult> {
        use encodings::*;
        use std::io::Write;

        let parsed_encodings = encodings::parse_accept_encoding(accept_encoding);
        let chosen_encoding =
            parsed_encodings
                .into_iter()
                .find_map(|(_group_weight, group_encodings)| {
                    for (enc, supported_enc) in SUPPORTED_ENCODING {
                        if group_encodings.contains(&enc) {
                            return Some(supported_enc);
                        }
                    }
                    None
                });
        let chosen_encoding = match chosen_encoding {
            Some(encoding) => encoding,
            None => {
                return Ok(CompressResult {
                    content_encoding: None,
                    payload: input,
                });
            }
        };
        match chosen_encoding {
            SupportedEncoding::Gzip => {
                let mut encoder =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                encoder.write_all(&input)?;
                let inner = encoder.finish()?;
                Ok(CompressResult {
                    content_encoding: Some("gzip"),
                    payload: inner.into(),
                })
            }
            SupportedEncoding::Deflate => {
                let mut encoder =
                    flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::fast());
                encoder.write_all(&input)?;
                let inner = encoder.finish()?;
                Ok(CompressResult {
                    content_encoding: Some("deflate"),
                    payload: inner.into(),
                })
            }
            SupportedEncoding::Brotli => {
                let mut encoder = brotli::CompressorWriter::new(Vec::new(), 4096, 0, 20);
                encoder.write_all(&input)?;
                encoder.flush()?;
                Ok(CompressResult {
                    content_encoding: Some("br"),
                    payload: encoder.into_inner().into(),
                })
            }
        }
    }
}

pub(crate) mod encodings {
    use std::{
        collections::{HashMap, HashSet},
        convert::Infallible,
        str::FromStr,
    };

    #[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
    pub(crate) enum SupportedEncoding {
        Gzip,
        Deflate,
        Brotli,
    }

    #[derive(Debug, PartialEq, Eq, Clone, Copy, Hash)]
    pub(crate) enum Encoding {
        Gzip,
        Compress,
        Deflate,
        Brotli,
        Zstd,
        Identity,
        Any,
        Unknown,
    }

    pub(crate) const SUPPORTED_ENCODING: [(Encoding, SupportedEncoding); 3] = [
        (Encoding::Brotli, SupportedEncoding::Brotli),
        (Encoding::Gzip, SupportedEncoding::Gzip),
        (Encoding::Deflate, SupportedEncoding::Deflate),
    ];

    impl FromStr for Encoding {
        type Err = Infallible;

        fn from_str(s: &str) -> Result<Self, Self::Err> {
            match s.trim().to_lowercase().as_str() {
                "gzip" => Ok(Encoding::Gzip),
                "compress" => Ok(Encoding::Compress),
                "deflate" => Ok(Encoding::Deflate),
                "br" => Ok(Encoding::Brotli),
                "zstd" => Ok(Encoding::Zstd),
                "identity" => Ok(Encoding::Identity),
                "*" => Ok(Encoding::Any),
                _ => Ok(Encoding::Unknown),
            }
        }
    }

    // Note: we handle weights as integers from [0 to 1000000], because floats
    // don't implement Eq / Ord
    #[derive(Debug, PartialEq, Eq, Clone, Copy, Hash, PartialOrd, Ord)]
    pub(crate) struct Weight(u32);

    impl From<f32> for Weight {
        fn from(f: f32) -> Self {
            let weight = (f * 1000000.0) as u32;
            Self(weight)
        }
    }

    pub(crate) fn parse_accept_encoding(header: &str) -> Vec<(Weight, HashSet<Encoding>)> {
        let mut groups: HashMap<Weight, HashSet<Encoding>> = Default::default();

        for (encoding, weight) in header.split(',').filter_map(|part| {
            let mut parts = part.trim().splitn(2, ';');
            let encoding: Encoding = parts.next()?.trim().parse().ok()?;
            let weight = parts
                .next()
                .and_then(|q| q.trim().trim_start_matches("q=").parse().ok())
                .unwrap_or(1.0);
            Some((encoding, weight))
        }) {
            let weight = Weight::from(weight);
            let group = groups.entry(weight).or_default();
            group.insert(encoding);
        }

        let mut v: Vec<_> = groups.into_iter().collect();
        // sort by weight, descending
        v.sort_by_key(|(weight, _)| std::cmp::Reverse(*weight));
        v
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn test_parse_accept_encoding() {
            assert_eq!(
                parse_accept_encoding("gzip, deflate"),
                vec![(Weight(1000000), [Encoding::Gzip, Encoding::Deflate].into())]
            );

            assert_eq!(
                parse_accept_encoding("gzip;q=1.0, identity; q=0.5, *;q=0"),
                vec![
                    (Weight(1000000), [Encoding::Gzip].into()),
                    (Weight(500000), [Encoding::Identity].into()),
                    (Weight(0), [Encoding::Any].into())
                ]
            );

            assert_eq!(
                parse_accept_encoding("deflate, gzip;q=1.0, *;q=0.5"),
                vec![
                    (Weight(1000000), [Encoding::Deflate, Encoding::Gzip].into()),
                    (Weight(500000), [Encoding::Any].into())
                ]
            );

            assert_eq!(
                parse_accept_encoding("br;q=1.0, gzip;q=0.8, *;q=0.1"),
                vec![
                    (Weight(1000000), [Encoding::Brotli].into()),
                    (Weight(800000), [Encoding::Gzip].into()),
                    (Weight(100000), [Encoding::Any].into())
                ]
            );

            assert_eq!(
                parse_accept_encoding(""),
                vec![(Weight(1000000), [Encoding::Unknown].into())]
            );

            assert_eq!(
                parse_accept_encoding("unknown"),
                vec![(Weight(1000000), [Encoding::Unknown].into())]
            );
        }
    }
}
