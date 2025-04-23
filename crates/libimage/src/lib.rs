include!(".dylo/spec.rs");
include!(".dylo/support.rs");

use ordered_float::OrderedFloat;
#[cfg(feature = "impl")]
use std::{io::Write, time::Instant};

use std::fmt;

use noteyre::BS;

use content_type::ContentType;
#[cfg(feature = "impl")]
use noteyre::BsForResults;

#[cfg(feature = "impl")]
use image::{DynamicImage, ImageDecoder, Rgb, Rgba};
#[cfg(feature = "impl")]
use jpegxl_rs::encode::EncoderFrame;
#[cfg(feature = "impl")]
use jxl_oxide::JxlImage;
#[cfg(feature = "impl")]
use rgb::FromSlice;

#[cfg(feature = "impl")]
#[derive(Default)]
struct ModImpl;

pub type Result<T, E = noteyre::BS> = std::result::Result<T, E>;

#[dylo::export]
impl Mod for ModImpl {
    fn transcode(
        &self,
        input: &[u8],
        ifmt: ICodec,
        ofmt: ICodec,
        target_width: Option<IntrinsicPixels>,
    ) -> Result<Vec<u8>> {
        let start_load = Instant::now();

        // Load the image from the input bytes
        let mut img = match ifmt {
            ICodec::PNG => {
                image::load_from_memory_with_format(input, image::ImageFormat::Png).bs()?
            }
            ICodec::JPG => {
                image::load_from_memory_with_format(input, image::ImageFormat::Jpeg).bs()?
            }
            ICodec::WEBP => {
                image::load_from_memory_with_format(input, image::ImageFormat::WebP).bs()?
            }
            ICodec::AVIF => {
                image::load_from_memory_with_format(input, image::ImageFormat::Avif).bs()?
            }
            ICodec::JXL => {
                let image = JxlImage::builder()
                    .read(input)
                    .map_err(|e| BS::from_string(format!("jxl decoding error: {e}")))?;
                let fb = image
                    .render_frame(0)
                    .map_err(|e| BS::from_string(format!("jxl rendering error: {e}")))?
                    .image();
                match fb.channels() {
                    3 => DynamicImage::from(
                        image::ImageBuffer::<Rgb<f32>, Vec<f32>>::from_raw(
                            fb.width() as u32,
                            fb.height() as u32,
                            fb.buf().to_vec(),
                        )
                        .ok_or_else(|| {
                            BS::from_string(
                                "failed to create ImageBuffer from jxl frame".to_owned(),
                            )
                        })?,
                    ),
                    4 => DynamicImage::from(
                        image::ImageBuffer::<Rgba<f32>, Vec<f32>>::from_raw(
                            fb.width() as u32,
                            fb.height() as u32,
                            fb.buf().to_vec(),
                        )
                        .ok_or_else(|| {
                            BS::from_string(
                                "failed to create ImageBuffer from jxl frame".to_owned(),
                            )
                        })?,
                    ),
                    _ => {
                        unimplemented!(
                            "unsupported number of channels in jxl image: {}",
                            fb.channels()
                        )
                    }
                }
            }
            ICodec::HEIC => {
                let mut temp_heic = tempfile::NamedTempFile::new().bs()?;
                temp_heic.write_all(input).bs()?;

                let temp_png = tempfile::NamedTempFile::new().bs()?;
                let temp_png_path = temp_png.path().to_str().unwrap();

                let status = std::process::Command::new("magick")
                    .arg(temp_heic.path())
                    .arg(format!("png:{temp_png_path}"))
                    .status()
                    .bs()?;

                if !status.success() {
                    return Err(BS::from_string("imagemagick convert failed".to_owned()));
                }

                image::load_from_memory_with_format(
                    fs_err::read(temp_png_path).bs()?.as_slice(),
                    image::ImageFormat::Png,
                )
                .bs()?
            }
        };

        let duration_load = start_load.elapsed();

        let start_resize = Instant::now();
        if let Some(target_width) = target_width {
            // The image::imageops::resize() function preserves aspect ratio while scaling
            // to the maximum size that fits within the given width/height bounds. So we
            // can just pass the original image height - the output will maintain aspect
            // ratio while ensuring width == target_width.
            img = img.resize(
                target_width.into_inner(),
                img.height(),
                image::imageops::FilterType::Lanczos3,
            );
        }
        let duration_resize = start_resize.elapsed();

        let start_transcode = Instant::now();

        // Encode the image into the output format
        let vec = match ofmt {
            ICodec::AVIF => {
                let encoder = ravif::Encoder::new()
                    .with_quality(85.0)
                    .with_alpha_quality(85.0)
                    .with_num_threads(Some(num_cpus::get()))
                    .with_speed(4); // 3 is _really slow_ (15 seconds on brat!)

                let res = if img.color().has_alpha() {
                    // not as, because it might be a non-u8 subpixel format
                    let rgba = img.to_rgba8();
                    let img = ravif::Img::new(
                        rgba.as_raw().as_rgba(),
                        img.width() as _,
                        img.height() as _,
                    );
                    encoder
                        .encode_rgba(img)
                        .map_err(|e| BS::from_string(format!("ravif error: {e}")))?
                } else {
                    let rgb = img.to_rgb8();
                    let img =
                        ravif::Img::new(rgb.as_raw().as_rgb(), img.width() as _, img.height() as _);
                    encoder
                        .encode_rgb(img)
                        .map_err(|e| BS::from_string(format!("ravif error: {e}")))?
                };

                res.avif_file
            }
            ICodec::WEBP => {
                // the WebP encoder only supports RGBA
                let img = img.to_rgba8();
                let img = DynamicImage::from(img);
                webp::Encoder::from_image(&img)
                    .map_err(|e| BS::from_string(format!("webp error: {}", e)))?
                    .encode(82.0)
                    .to_vec()
            }
            ICodec::PNG => {
                use image::ImageEncoder as _;
                let mut bytes: Vec<u8> = Vec::new();
                let img = img.to_rgba8(); // sometimes we get Rgb32F (out of a JPEG-XL) and PNG does _not_ like that.
                let img = DynamicImage::from(img);
                image::codecs::png::PngEncoder::new(&mut bytes)
                    .write_image(
                        img.as_bytes(),
                        img.width(),
                        img.height(),
                        img.color().into(),
                    )
                    .bs()?;
                bytes
            }
            ICodec::JXL => {
                let runner = jpegxl_rs::ThreadsRunner::default();

                let mut encoder = jpegxl_rs::encoder_builder()
                    .parallel_runner(&runner)
                    .quality(2.8) // that's distance, actually (lower is better)
                    .speed(jpegxl_rs::encode::EncoderSpeed::Squirrel) // effort, 7
                    .build()
                    .bs()?;

                // Handle RGB and RGBA cases separately
                if img.color().has_alpha() {
                    let rgba = img.to_rgba8();
                    encoder.has_alpha = true;
                    let frame = EncoderFrame::new(rgba.as_raw()).num_channels(4);
                    encoder
                        .encode_frame::<_, u8>(&frame, img.width(), img.height())
                        .bs()?
                        .data
                } else {
                    let rgb = img.to_rgb8();
                    encoder.has_alpha = false;
                    let frame = EncoderFrame::new(rgb.as_raw()).num_channels(3);
                    encoder
                        .encode_frame::<_, u8>(&frame, img.width(), img.height())
                        .bs()?
                        .data
                }
            }
            _ => {
                return Err(BS::from_string(format!(
                    "unsupported image format: {:?}",
                    ofmt
                )))
            }
        };

        let duration_transcode = start_transcode.elapsed();
        tracing::info!(
            "\x1b[36m{:?}\x1b[0m => \x1b[36m{:?}\x1b[0m, load: \x1b[33m{:?}\x1b[0m, resize: \x1b[33m{:?}\x1b[0m, transcode: \x1b[33m{:?}\x1b[0m, total: \x1b[33m{:?}\x1b[0m",
            ifmt,
            ofmt,
            duration_load,
            duration_resize,
            duration_transcode,
            duration_load + duration_resize + duration_transcode
        );

        Ok(vec)
    }

    fn dimensions(&self, input: &[u8], ifmt: ICodec) -> Result<(IntrinsicPixels, IntrinsicPixels)> {
        let input = std::io::Cursor::new(input);

        let (width, height) = match ifmt {
            ICodec::PNG => {
                let decoder = image::codecs::png::PngDecoder::new(input).bs()?;
                decoder.dimensions()
            }
            ICodec::JPG => {
                let decoder = image::codecs::jpeg::JpegDecoder::new(input).bs()?;
                decoder.dimensions()
            }
            ICodec::WEBP => {
                let decoder = image::codecs::webp::WebPDecoder::new(input).bs()?;
                decoder.dimensions()
            }
            ICodec::AVIF => {
                let decoder = image::codecs::avif::AvifDecoder::new(input).bs()?;
                decoder.dimensions()
            }
            ICodec::JXL => {
                let image = JxlImage::builder()
                    .read(input)
                    .map_err(|e| BS::from_string(format!("jxl decoding error: {e}")))?;
                (image.width(), image.height())
            }
            ICodec::HEIC => {
                return Err(BS::from_string(
                    "heic dimensions: unsupported :(".to_owned(),
                ))
            }
        };
        Ok((IntrinsicPixels::from(width), IntrinsicPixels::from(height)))
    }
}
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
            type Error = BS;

            fn try_from(ct: ContentType) -> Result<Self, Self::Error> {
                match ct {
                    $(ct if ct == $content_type => Ok(ICodec::$variant),)*
                    _ => Err(BS::from_string(format!("Unknown image codec for content type: {}", ct))),
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
