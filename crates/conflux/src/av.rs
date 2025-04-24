use content_type::ContentType;
use libimage::{ICodec, IntrinsicPixels, LogicalPixels, PixelDensity};

use crate::{
    ContentTypeCodecRef, FfmpegChannels, FfmpegCodec, FfmpegCodecRef, FfmpegPixelFormat, InputPath,
    InputPathRef, Route,
};

/// A media container we know about
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VContainer {
    /// ISO Base Media File Format (typically .mp4)
    MP4,
    /// WebM container format
    WebM,
}

merde::derive! {
    impl (Serialize, Deserialize) for enum VContainer string_like {
        "mp4" => MP4,
        "webm" => WebM,
    }
}

impl VContainer {
    pub fn content_type(&self) -> ContentType {
        match self {
            VContainer::MP4 => ContentType::MP4,
            VContainer::WebM => ContentType::WebM,
        }
    }
}

impl std::fmt::Display for VContainer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            VContainer::MP4 => write!(f, "mp4"),
            VContainer::WebM => write!(f, "webm"),
        }
    }
}

/// A video codec we know about
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(clippy::upper_case_acronyms)]
pub enum VCodec {
    AVC,
    VP9,
    AV1,
}

macro_rules! impl_codec_conversions {
    ($codec:tt, $($variant:ident => $ffmpeg_name:expr),*) => {
        impl TryFrom<&FfmpegCodecRef> for $codec {
            type Error = ();

            fn try_from(value: &FfmpegCodecRef) -> Result<Self, Self::Error> {
                match value.as_str() {
                    $($ffmpeg_name => Ok(<$codec>::$variant),)*
                    _ => Err(()),
                }
            }
        }

        impl From<$codec> for &'static FfmpegCodecRef {
            fn from(value: $codec) -> Self {
                FfmpegCodecRef::from_static(match value {
                    $(<$codec>::$variant => $ffmpeg_name,)*
                })
            }
        }

        impl $codec {
            pub fn ffmpeg_codec_name(self) -> &'static FfmpegCodecRef {
                self.into()
            }
        }

        impl std::fmt::Display for $codec {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.ffmpeg_codec_name())
            }
        }

        merde::derive! {
            impl (Serialize, Deserialize) for enum $codec string_like {
                $($ffmpeg_name => $variant,)*
            }
        }
    };
}

impl_codec_conversions!(VCodec,
    AVC => "h264",
    VP9 => "vp9",
    AV1 => "av1"
);

impl VCodec {
    pub fn content_type_codec(&self) -> &ContentTypeCodecRef {
        match self {
            // av01 = AV1 codec
            // 0 = profile (main)
            // 08 = level (2.3)
            // M = tier (main)
            // 08 = bit depth (8 bits)
            VCodec::AV1 => ContentTypeCodecRef::from_static("av01.0.08M.08"),

            // avc1 = H.264/AVC codec
            // 64 = profile (high)
            // 00 = constraint flags
            // 34 = level (5.0)
            VCodec::AVC => ContentTypeCodecRef::from_static("avc1.640034"),

            // vp09 = VP9 codec
            // 00 = profile (0)
            // 41 = level (4.1)
            // 08 = bit depth (8 bits)
            VCodec::VP9 => ContentTypeCodecRef::from_static("vp09.00.41.08"),
        }
    }
}

/// An audio codec we know about
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ACodec {
    Opus,
    Aac,
}

impl_codec_conversions!(ACodec,
    Opus => "opus",
    Aac => "aac"
);

impl ACodec {
    pub fn content_type_codec_name(&self) -> &'static str {
        match self {
            // Opus doesn't need any extra parameters because it's already a well-defined codec
            ACodec::Opus => "opus",

            // mp4a = MPEG-4 Audio
            // 40 = Audio Object Type (AAC-LC)
            // 2 = Frequency index (see ISO 14496-3)
            ACodec::Aac => "mp4a.40.2",
        }
    }
}

#[derive(Debug, Clone)]
pub struct MediaProps {
    /// The kind of media this is
    pub kind: MediaKind,

    /// Dimensions (width, height, and pixel density)
    pub dims: Dimensions,

    /// Duration in seconds, e.g. 123.456.
    /// Anything static has this hardcoded to 1.0 second.
    pub secs: f64,

    pub ic: Option<ICodec>,
    pub vp: Option<VParams>,
    pub ap: Option<AParams>,
}

impl MediaProps {
    pub fn new(kind: MediaKind, dimensions: Dimensions, duration: f64) -> Self {
        Self {
            kind,
            dims: dimensions,
            secs: duration,
            ic: None,
            vp: None,
            ap: None,
        }
    }

    pub fn ac(&self) -> Option<ACodec> {
        let codec = self.ap.as_ref()?.codec.as_deref()?;
        ACodec::try_from(codec).ok()
    }

    pub fn vc(&self) -> Option<VCodec> {
        let codec = self.vp.as_ref()?.codec.as_deref()?;
        VCodec::try_from(codec).ok()
    }
}

merde::derive! {
    impl (Serialize, Deserialize) for struct MediaProps {
        kind,
        dims,
        secs,
        ic,
        vp,
        ap
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Dimensions {
    pub w: IntrinsicPixels,
    pub h: IntrinsicPixels,
    pub density: PixelDensity,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Dimensions {
        w, h, density
    }
}

#[derive(Clone, Debug)]
pub struct BitmapVariant {
    pub ic: ICodec,

    /// this is for a CSS media query in the `media` attribute of a source
    pub max_width: Option<LogicalPixels>,

    pub srcset: Vec<(PixelDensity, Route)>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct BitmapVariant {
        ic, max_width, srcset
    }
}

/// A container, audio codec and video codec combination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoVariant {
    pub route: Route,
    pub container: VContainer,
    pub ac: ACodec,
    pub vc: VCodec,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct VideoVariant {
        route, container, ac, vc
    }
}

impl VideoVariant {
    /// Build the input path" for this video variant — it'll include the video codec, audio codec,
    /// and container.
    pub fn path(&self, original_asset: &InputPathRef) -> InputPath {
        let (base, _ext) = original_asset.explode();
        InputPath::new(format!("{base}.{ext}", ext = self.ext()))
    }

    /// Build the file extension for this video variant
    pub fn ext(&self) -> String {
        format!(
            "{vc}+{ac}.{fmt}",
            vc = self.vc,
            ac = self.ac,
            fmt = self.container.content_type().ext(),
        )
    }
}

/// A thumbnail for a video
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VideoThumbnail {
    pub route: Route,
    pub fmt: ICodec,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct VideoThumbnail {
        route, fmt
    }
}

impl VideoThumbnail {
    /// Build the "input path" for this video thumbnail
    pub fn path(&self, original_asset: &InputPathRef) -> InputPath {
        let (base, _ext) = original_asset.explode();
        InputPath::new(format!("{base}.{}", self.ext()))
    }

    /// Get the file extension for this video thumbnail
    pub fn ext(&self) -> String {
        format!("thumb.{}", self.fmt.ext())
    }
}

pub struct VideoVariantContentType<'a>(&'a VideoVariant);

impl std::fmt::Display for VideoVariantContentType<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}; codecs={},{}",
            self.0.container.content_type(),
            self.0.vc.content_type_codec(),
            self.0.ac.content_type_codec_name()
        )
    }
}

impl VideoVariant {
    /// Returns a struct that implements Display for the full content-type header value
    pub fn qualified_content_type(&self) -> VideoVariantContentType {
        VideoVariantContentType(self)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MediaKind {
    /// Stored as JXL
    Bitmap,
    /// Stored as AV1
    Video,
    /// Stored as Opus, ideally? For now just M4A files with AAC in them
    Audio,
    /// Stored as .drawio files
    Diagram,
}

merde::derive! {
    impl (Serialize, Deserialize) for enum MediaKind string_like {
        "image" => Bitmap,
        "video" => Video,
        "audio" => Audio,
        "diagram" => Diagram,
    }
}

#[derive(Clone, Debug)]
pub struct VParams {
    /// Video codec name from FFmpeg, e.g. "h264", "av1", "vp9"
    pub codec: Option<FfmpegCodec>,
    /// Frames per second, e.g. 29.97, 30.0, 59.94
    pub frame_rate: Option<f64>,
    /// Pixel format from FFmpeg, e.g. "yuv420p", "yuv444p"
    pub pix_fmt: Option<FfmpegPixelFormat>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct VParams {
        codec, frame_rate, pix_fmt
    }
}

#[derive(Clone, Debug)]
pub struct AParams {
    /// Audio codec name from FFmpeg, e.g. "aac", "opus"
    pub codec: Option<FfmpegCodec>,
    /// Audio sample rate in Hz, e.g. 44100, 48000
    pub sample_rate: Option<u32>,
    /// Audio channels, e.g. `stereo`, `5.1` or `7.1`
    pub channels: Option<FfmpegChannels>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct AParams {
        codec, sample_rate, channels
    }
}

#[derive(Clone, Debug)]
pub struct Media {
    pub props: MediaProps,

    /// bitmap variants (`<source>` in `<picture>`)
    pub bv: Vec<BitmapVariant>,

    /// video variants (`<source>` in `<video>`)
    pub vv: Vec<VideoVariant>,

    /// route for accept-based-redirected thumbnail
    pub thumb: Option<Route>,
}

impl Media {
    /// Build a new media
    pub fn new(props: MediaProps) -> Self {
        Self {
            props,
            bv: vec![],
            vv: vec![],
            thumb: None,
        }
    }

    /// Set bitmap variant
    pub fn with_bitmap_variant(mut self, variant: BitmapVariant) -> Self {
        self.bv.push(variant);
        self
    }

    /// Set video variants
    pub fn with_video_variant(mut self, variant: VideoVariant) -> Self {
        self.vv.push(variant);
        self
    }

    pub fn acodec(&self) -> Option<ACodec> {
        self.props.ac()
    }

    pub fn vcodec(&self) -> Option<VCodec> {
        self.props.vc()
    }
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Media {
        props, bv, vv, thumb
    }
}
