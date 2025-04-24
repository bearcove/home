use content_type::ContentType;
use ordered_float::OrderedFloat;
use std::fmt;

macro_rules! define_icodec {
    ($($variant:ident => ($ser_str:expr, $ffmpeg_name:expr, $content_type:expr, $ext:expr, $content_type_pattern:expr)),* $(,)?) => {
        /// An image format we know about
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        #[allow(clippy::upper_case_acronyms)]
        pub enum ICodec {
            $($variant),*
        }

        merde::derive! {
            impl (Serialize, Deserialize) for enum ICodec string_like {
                $($ser_str => $variant),*
            }
        }

        impl ICodec {
            pub fn from_ffmpeg_codec_name(name: &str) -> Option<Self> {
                match name {
                    $($ffmpeg_name => Some(ICodec::$variant),)*
                    _ => None,
                }
            }

            /// Return the image extension (e.g. "png")
            pub fn ext(self) -> &'static str {
                match self {
                    $(ICodec::$variant => $ext),*
                }
            }

            /// Return the image content type (e.g. "image/png")
            pub fn content_type(self) -> ContentType {
                match self {
                    $(ICodec::$variant => $content_type),*
                }
            }

            /// Guess the image format from a content type
            pub fn from_content_type_str(content_type: &str) -> Option<Self> {
                match content_type {
                    $(ct if ct.starts_with($content_type_pattern) => Some(ICodec::$variant),)*
                    _ => None,
                }
            }
        }

        impl TryFrom<ContentType> for ICodec {
            type Error = eyre::Report;

            fn try_from(ct: ContentType) -> Result<Self, Self::Error> {
                match ct {
                    $(ct if ct == $content_type => Ok(ICodec::$variant),)*
                    _ => Err(eyre::eyre!("Unknown image codec for content type: {}", ct)),
                }
            }
        }

        impl std::str::FromStr for ICodec {
            type Err = String;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                match s.to_lowercase().as_str() {
                    $(
                        $ser_str => Ok(ICodec::$variant),
                    )*
                    _ => Err(format!("Unknown image codec: {}", s)),
                }
            }
        }

        impl std::fmt::Display for ICodec {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                match self {
                    $(ICodec::$variant => write!(f, $ser_str),)*
                }
            }
        }
    };
}

define_icodec! {
    PNG => ("png", "png", ContentType::PNG, "png", "image/png"),
    WEBP => ("webp", "webp", ContentType::WEBP, "webp", "image/webp"),
    AVIF => ("avif", "avif", ContentType::AVIF, "avif", "image/avif"),
    JPG => ("jpg", "mjpeg", ContentType::JPG, "jpg", "image/jpeg"),
    JXL => ("jxl", "jpegxl", ContentType::JXL, "jxl", "image/jxl"),
    HEIC => ("heic", "hevc", ContentType::HEIC, "heic", "image/heic"),
}

macro_rules! u32_wrapper {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[repr(transparent)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(u32);

        impl $name {
            pub fn into_inner(&self) -> u32 {
                self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<u32> for $name {
            fn from(v: u32) -> Self {
                Self(v)
            }
        }

        impl From<$name> for u32 {
            fn from(v: $name) -> Self {
                v.0
            }
        }

        merde::derive! {
            impl (Serialize, Deserialize) for struct $name transparent
        }
    }
}

macro_rules! ordered_f32_wrapper {
    ($(#[$attr:meta])* $name:ident) => {
        $(#[$attr])*
        #[repr(transparent)]
        #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(OrderedFloat<f32>);

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0.into_inner())
            }
        }

        impl $name {
            pub fn into_inner(&self) -> f32 {
                self.0.into_inner()
            }
        }

        impl From<f32> for $name {
            fn from(v: f32) -> Self {
                Self(OrderedFloat(v))
            }
        }

        impl From<$name> for f32 {
            fn from(v: $name) -> Self {
                v.0.into_inner()
            }
        }

        impl merde::Serialize for $name {
            async fn serialize<'fut>(
                &'fut self,
                ser: &'fut mut dyn merde::DynSerializer,
            ) -> Result<(), merde::MerdeError<'static>> {
                self.0.into_inner().serialize(ser).await
            }
        }

        impl<'s> merde::Deserialize<'s> for $name {
            async fn deserialize<'de>(
                de: &'de mut dyn merde::DynDeserializer<'s>,
            ) -> Result<Self, merde::MerdeError<'s>> {
                let v = f32::deserialize(de).await?;
                Ok(Self(OrderedFloat(v)))
            }
        }
    }
}

// "physical" pixel, an `800px@2` image has 1600 of them.
u32_wrapper!(IntrinsicPixels);

impl IntrinsicPixels {
    /// Convert to physical pixels
    pub fn to_logical(&self, density: PixelDensity) -> LogicalPixels {
        LogicalPixels::from(self.0 as f32 / density.0.into_inner())
    }
}

// CSS `px` unit, an `800px@2` image has 800 of them.
ordered_f32_wrapper!(LogicalPixels);

impl LogicalPixels {
    /// Convert to intrinsic pixels at a given density
    pub fn to_intrinsic(&self, density: PixelDensity) -> IntrinsicPixels {
        IntrinsicPixels::from((self.0 * density.0).into_inner() as u32)
    }
}

// Pixel density (e.g. 1.0 for normal display, 2.0 for retina)
ordered_f32_wrapper!(PixelDensity);

impl PixelDensity {
    pub const ONE: PixelDensity = PixelDensity(OrderedFloat(1.0));
    pub const TWO: PixelDensity = PixelDensity(OrderedFloat(2.0));
}
