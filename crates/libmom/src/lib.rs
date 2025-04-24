use camino::Utf8PathBuf;
use config::MomConfig;
use config::TenantConfig;
use config::TenantDomain;
use config::TenantInfo;
use config::WebConfig;
use conflux::Derivation;
use conflux::DerivationHash;
use conflux::Input;
use conflux::{InputPath, Pak};
use derivations::DerivationInfo;
use eyre::BS;
use futures_core::future::BoxFuture;
use media_types::TargetFormat;
use media_types::TranscodingProgress;
use merde::CowStr;
use objectstore::ObjectStoreKey;
use std::time::Instant;
use std::{collections::HashMap, sync::Arc};

#[derive(Default)]
struct ModImpl;

pub type Result<T, E = eyre::BS> = std::result::Result<T, E>;

pub struct MomServeArgs {
    pub config: MomConfig,
    pub web: WebConfig,
    pub tenants: HashMap<TenantDomain, TenantInfo>,
    pub listener: tokio::net::TcpListener,
}

#[autotrait]
impl Mod for ModImpl {
    fn serve(&self, args: MomServeArgs) -> BoxFuture<Result<(), BS>> {
        Box::pin(impls::serve(args))
    }
}

pub(crate) mod impls;

#[derive(Clone)]
#[cfg_attr(feature = "serde", derive(serde::Serialize))]
pub struct Sponsors<'s> {
    pub sponsors: Vec<CowStr<'s>>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Sponsors<'s> { sponsors }
}

#[derive(Debug, Clone)]
pub struct ListMissingArgs {
    /// queries if ObjectStoreKey is in object storage, if
    /// not we'll return the InputPath
    pub objects_to_query: HashMap<ObjectStoreKey, InputPath>,

    /// this is only set when a local mom is hitting
    /// the production mom — to make sure the local
    /// info becomes remote, too.
    pub mark_these_as_uploaded: Option<Vec<ObjectStoreKey>>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct ListMissingArgs { objects_to_query, mark_these_as_uploaded }
}

#[derive(Debug, Clone)]
pub struct ListMissingResponse {
    pub missing: HashMap<ObjectStoreKey, InputPath>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct ListMissingResponse { missing }
}

#[derive(Debug)]
pub enum MomEvent<'s> {
    GoodMorning(GoodMorning<'s>),
    TenantEvent(TenantEvent<'s>),
}

merde::derive! {
    impl (Serialize, Deserialize) for enum MomEvent<'s>
    externally_tagged
    {
        "GoodMorning" => GoodMorning,
        "TenantEvent" => TenantEvent,
    }
}

#[derive(Debug)]
pub struct TenantEvent<'s> {
    pub tenant_name: TenantDomain,
    pub payload: TenantEventPayload<'s>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct TenantEvent<'s> { tenant_name, payload }
}

pub enum TenantEventPayload<'s> {
    RevisionChanged(Box<Pak<'s>>),
    SponsorsUpdated(Sponsors<'s>),
}

impl std::fmt::Debug for TenantEventPayload<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TenantEventPayload::RevisionChanged(revision) => write!(
                f,
                "TenantEvent::RevisionChanged({}, {} inputs, {} pages, {} templates, {} media)",
                revision.id,
                revision.inputs.len(),
                revision.pages.len(),
                revision.templates.len(),
                revision.media_props.len(),
            ),
            TenantEventPayload::SponsorsUpdated(sponsors) => write!(
                f,
                "TenantEvent::SponsorsUpdated({} sponsors)",
                sponsors.sponsors.len()
            ),
        }
    }
}

merde::derive! {
    impl (Serialize, Deserialize) for enum TenantEventPayload<'s>
    externally_tagged
    {
        "RevisionChanged" => RevisionChanged,
        "SponsorsUpdated" => SponsorsUpdated,
    }
}

#[derive(Debug)]
pub struct GoodMorning<'s> {
    pub initial_states: HashMap<TenantDomain, TenantInitialState<'s>>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct GoodMorning<'s> { initial_states }
}

pub struct TenantInitialState<'s> {
    /// The revision to serve (if any)
    pub pak: Option<Pak<'s>>,

    /// The sponsors for this tenant
    pub sponsors: Option<Sponsors<'s>>,

    /// The configuration for this tenant
    pub tc: TenantConfig,

    /// if mom and cub are colocated, they can share a data dir (especially important in dev)
    pub base_dir: Option<Utf8PathBuf>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct TenantInitialState<'s> { pak, sponsors, tc, base_dir }
}

impl std::fmt::Debug for TenantInitialState<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantInitialState").finish_non_exhaustive()
    }
}

pub trait GlobalStateView: Send + Sync + 'static {
    fn gsv_sponsors(&self) -> Arc<Sponsors<'static>> {
        unimplemented!()
    }

    fn gsv_ti(&self) -> Arc<TenantInfo> {
        unimplemented!()
    }
}

#[derive(Debug, Clone)]
pub struct TranscodeJobInfo {
    pub started: Instant,
    pub last_ping: Instant,
    pub last_progress: Option<TranscodingProgress>,
}

// Note: this is tenant-specific, the video data etc. is per-tenant.
#[derive(PartialEq, Eq, Debug, Clone, Hash)]
pub struct TranscodeParams {
    // source data
    pub input: ObjectStoreKey,

    // target format
    pub target_format: TargetFormat,

    // target object key
    pub output: ObjectStoreKey,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct TranscodeParams { input, target_format, output }
}

pub enum TranscodeResponse {
    Done(TranscodeResponseDone),
    AlreadyInProgress(TranscodeResponseAlreadyInProgress),
    TooManyRequests(TranscodeResponseTooManyRequests),
}

merde::derive! {
    impl (Serialize, Deserialize) for enum TranscodeResponse
    externally_tagged
    {
        "Done" => Done,
        "AlreadyInProgress" => AlreadyInProgress,
        "TooManyRequests" => TooManyRequests,
    }
}

pub struct TranscodeResponseDone {
    pub output_size: usize,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct TranscodeResponseDone { output_size }
}

#[derive(Debug)]
pub struct TranscodeResponseAlreadyInProgress {
    pub info: String,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct TranscodeResponseAlreadyInProgress { info }
}

pub struct TranscodeResponseTooManyRequests {}

merde::derive! {
    impl (Serialize, Deserialize) for struct TranscodeResponseTooManyRequests {}
}

#[derive(Debug, Clone)]
pub struct DeriveJobInfo {
    pub started: Instant,
    pub last_ping: Instant,
    pub last_progress: Option<TranscodingProgress>,
}

#[derive(Debug, Clone)]
pub struct DeriveParams {
    // input for the derivation
    pub input: Input,

    // derivation to compute
    pub derivation: Derivation,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct DeriveParams { input, derivation }
}

impl DeriveParams {
    /// The output hash
    fn hash(&self) -> DerivationHash {
        DerivationInfo::new(&self.input, &self.derivation).hash()
    }
}

impl PartialEq for DeriveParams {
    fn eq(&self, other: &Self) -> bool {
        self.hash() == other.hash()
    }
}

impl Eq for DeriveParams {}

impl std::hash::Hash for DeriveParams {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.hash().hash(state)
    }
}

pub enum DeriveResponse {
    Done(DeriveResponseDone),
    AlreadyInProgress(DeriveResponseAlreadyInProgress),
    TooManyRequests(DeriveResponseTooManyRequests),
}

merde::derive! {
    impl (Serialize, Deserialize) for enum DeriveResponse
    externally_tagged
    {
        "Done" => Done,
        "AlreadyInProgress" => AlreadyInProgress,
        "TooManyRequests" => TooManyRequests,
    }
}

pub struct DeriveResponseDone {
    /// How large the output was
    pub output_size: usize,

    /// Where the output was stored
    pub dest: ObjectStoreKey,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct DeriveResponseDone { output_size, dest }
}

#[derive(Debug)]
pub struct DeriveResponseAlreadyInProgress {
    pub info: String,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct DeriveResponseAlreadyInProgress { info }
}

pub struct DeriveResponseTooManyRequests {}

merde::derive! {
    impl (Serialize, Deserialize) for struct DeriveResponseTooManyRequests {}
}

pub mod media_types {
    use conflux::{MediaProps, VCodec};
    use eyre::BS;
    use image::ICodec;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
    pub enum TargetFormat {
        AV1,
        VP9,
        ThumbJXL,
        ThumbAVIF,
        ThumbWEBP,
    }

    impl TargetFormat {
        pub fn as_thumb_format(&self) -> Option<ICodec> {
            match self {
                TargetFormat::ThumbJXL => Some(ICodec::JXL),
                TargetFormat::ThumbAVIF => Some(ICodec::AVIF),
                TargetFormat::ThumbWEBP => Some(ICodec::WEBP),
                _ => None,
            }
        }

        pub fn postprocess(&self) -> Option<PostProcess> {
            match self {
                TargetFormat::ThumbAVIF => Some(PostProcess {
                    src_ic: ICodec::JXL,
                    dst_ic: ICodec::AVIF,
                }),
                TargetFormat::ThumbWEBP => Some(PostProcess {
                    src_ic: ICodec::JXL,
                    dst_ic: ICodec::WEBP,
                }),
                _ => None,
            }
        }

        pub fn ffmpeg_output_ext(&self) -> &'static str {
            match self {
                TargetFormat::AV1 => "mp4",
                TargetFormat::VP9 => "webm",
                TargetFormat::ThumbJXL => "jxl",
                TargetFormat::ThumbAVIF => "jxl",
                TargetFormat::ThumbWEBP => "jxl",
            }
        }
    }

    pub struct PostProcess {
        pub src_ic: ICodec,
        pub dst_ic: ICodec,
    }

    merde::derive! {
        impl (Serialize, Deserialize) for enum TargetFormat string_like {
            "av1" => AV1,
            "vp9" => VP9,
            "thumb_jxl" => ThumbJXL,
            "thumb_avif" => ThumbAVIF,
            "thumb_webp" => ThumbWEBP,
        }
    }

    impl TryFrom<VCodec> for TargetFormat {
        type Error = BS;

        fn try_from(value: VCodec) -> Result<Self, Self::Error> {
            match value {
                VCodec::VP9 => Ok(TargetFormat::VP9),
                VCodec::AV1 => Ok(TargetFormat::AV1),
                format => Err(BS::from_string(format!("Refusing to encode to {format:?}"))),
            }
        }
    }

    impl TryFrom<ICodec> for TargetFormat {
        type Error = BS;

        fn try_from(value: ICodec) -> Result<Self, Self::Error> {
            match value {
                ICodec::AVIF => Ok(TargetFormat::ThumbAVIF),
                ICodec::WEBP => Ok(TargetFormat::ThumbWEBP),
                ICodec::JXL => Ok(TargetFormat::ThumbJXL),
                format => Err(BS::from_string(format!(
                    "Refusing to grab thumbnail in format {format:?}"
                ))),
            }
        }
    }

    #[derive(Debug)]
    pub enum WebSocketMessage {
        Headers(HeadersMessage),
        UploadDone(UploadDoneMessage),
        TranscodingEvent(TranscodeEvent),
        TranscodingComplete(TranscodingCompleteMessage),
        Error(String),
    }

    merde::derive! {
        impl (Serialize, Deserialize) for enum WebSocketMessage
        externally_tagged {
            "Headers" => Headers,
            "UploadDone" => UploadDone,
            "TranscodingEvent" => TranscodingEvent,
            "TranscodingComplete" => TranscodingComplete,
            "Error" => Error,
        }
    }

    #[derive(Debug)]
    pub struct HeadersMessage {
        pub target_format: TargetFormat,
        pub file_name: String,
        pub file_size: usize,
    }

    merde::derive! {
        impl (Serialize, Deserialize) for struct HeadersMessage {
            target_format,
            file_name,
            file_size
        }
    }

    #[derive(Debug)]
    pub struct UploadDoneMessage {
        pub uploaded_size: usize,
    }

    merde::derive! {
        impl (Serialize, Deserialize) for struct UploadDoneMessage { uploaded_size }
    }

    #[derive(Debug)]
    pub struct TranscodingCompleteMessage {
        pub output_size: usize,
    }

    merde::derive! {
        impl (Serialize, Deserialize) for struct TranscodingCompleteMessage { output_size }
    }

    #[derive(Debug, Clone)]
    pub struct TranscodingProgress {
        pub frame: u32,
        pub fps: f32,
        pub quality: f32,
        pub size_kb: u32,
        pub bitrate_kbps: f32,
        pub speed: f32,

        // in seconds
        pub processed_time: f64,
        // in seconds
        pub total_time: f64,
    }

    impl std::fmt::Display for TranscodingProgress {
        fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            write!(
                f,
                "Frame {}, FPS {:.2}, Quality {:.2}, Size {}kb, Time {:.2}/{:.2}s, Bitrate {:.2}kbps, Speed {:.2}x",
                self.frame,
                self.fps,
                self.quality,
                self.size_kb,
                self.processed_time,
                self.total_time,
                self.bitrate_kbps,
                self.speed
            )
        }
    }

    merde::derive! {
        impl (Serialize, Deserialize) for struct TranscodingProgress {
            frame,
            fps,
            quality,
            size_kb,
            bitrate_kbps,
            speed,
            processed_time,
            total_time
        }
    }

    #[derive(Debug, Clone)]
    pub enum TranscodeEvent {
        MediaIdentified(MediaProps),
        Progress(TranscodingProgress),
    }

    merde::derive! {
        impl (Serialize, Deserialize) for enum TranscodeEvent
        externally_tagged {
            "MediaIdentified" => MediaIdentified,
            "Progress" => Progress,
        }
    }
}
