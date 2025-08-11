use camino::Utf8PathBuf;
use conflux::{Derivation, DerivationHash, Input, InputPath, Pak};
use credentials::{UserId, UserInfo};
use derivations::DerivationInfo;
use facet::Facet;
use media_types::{TargetFormat, TranscodingProgress};
use objectstore_types::ObjectStoreKey;
use std::{collections::HashMap, sync::Arc, time::Instant};

use config_types::{MomConfig, TenantConfig, TenantDomain, TenantInfo, WebConfig};

pub trait GlobalStateView: Send + Sync + 'static {
    fn gsv_users(&self) -> Arc<AllUsers> {
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
#[derive(PartialEq, Eq, Debug, Clone, Hash, Facet)]
pub struct TranscodeParams {
    // source data
    pub input: ObjectStoreKey,

    // target format
    pub target_format: TargetFormat,

    // target object key
    pub output: ObjectStoreKey,
}

#[derive(Facet)]
#[repr(u8)]
pub enum TranscodeResponse {
    Done(TranscodeResponseDone),
    AlreadyInProgress(TranscodeResponseAlreadyInProgress),
    TooManyRequests(TranscodeResponseTooManyRequests),
}

#[derive(Facet)]
pub struct TranscodeResponseDone {
    pub output_size: usize,
}

#[derive(Debug, Facet)]
pub struct TranscodeResponseAlreadyInProgress {
    pub info: String,
}

#[derive(Facet)]
pub struct TranscodeResponseTooManyRequests {}

#[derive(Debug, Clone)]
pub struct DeriveJobInfo {
    pub started: Instant,
    pub last_ping: Instant,
    pub last_progress: Option<TranscodingProgress>,
}

#[derive(Facet, Debug, Clone)]
pub struct DeriveParams {
    // input for the derivation
    pub input: Input,

    // derivation to compute
    pub derivation: Derivation,
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

#[derive(Facet)]
#[repr(u8)]
pub enum DeriveResponse {
    Done(DeriveResponseDone),
    AlreadyInProgress(DeriveResponseAlreadyInProgress),
    TooManyRequests(DeriveResponseTooManyRequests),
}

#[derive(Facet)]
pub struct DeriveResponseDone {
    /// How large the output was
    pub output_size: usize,

    /// Where the output was stored
    pub dest: ObjectStoreKey,
}

#[derive(Debug, Facet)]
pub struct DeriveResponseAlreadyInProgress {
    pub info: String,
}

#[derive(Facet)]
pub struct DeriveResponseTooManyRequests {}

pub mod media_types {
    use conflux::{MediaProps, VCodec};
    use facet::Facet;
    use image_types::ICodec;

    #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Facet)]
    #[repr(u8)]
    pub enum TargetFormat {
        AV1,
        AVC,
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
                TargetFormat::AVC => "mp4",
                TargetFormat::VP9 => "webm",
                TargetFormat::ThumbJXL => "jxl",
                TargetFormat::ThumbAVIF => "jxl",
                TargetFormat::ThumbWEBP => "jxl",
            }
        }
    }

    #[derive(Facet)]
    pub struct PostProcess {
        pub src_ic: ICodec,
        pub dst_ic: ICodec,
    }

    impl TryFrom<VCodec> for TargetFormat {
        type Error = eyre::Report;

        fn try_from(value: VCodec) -> Result<Self, Self::Error> {
            match value {
                VCodec::VP9 => Ok(TargetFormat::VP9),
                VCodec::AV1 => Ok(TargetFormat::AV1),
                format => eyre::bail!("Refusing to encode to {format:?}"),
            }
        }
    }

    impl TryFrom<ICodec> for TargetFormat {
        type Error = eyre::Report;

        fn try_from(value: ICodec) -> Result<Self, Self::Error> {
            match value {
                ICodec::AVIF => Ok(TargetFormat::ThumbAVIF),
                ICodec::WEBP => Ok(TargetFormat::ThumbWEBP),
                ICodec::JXL => Ok(TargetFormat::ThumbJXL),
                format => eyre::bail!("Refusing to grab thumbnail in format {format:?}"),
            }
        }
    }

    #[derive(Debug, Facet)]
    #[repr(u8)]
    pub enum WebSocketMessage {
        Headers(HeadersMessage),
        UploadDone(UploadDoneMessage),
        TranscodingEvent(TranscodeEvent),
        TranscodingComplete(TranscodingCompleteMessage),
        Error(String),
    }

    #[derive(Debug, Facet)]
    pub struct HeadersMessage {
        pub target_format: TargetFormat,
        pub file_name: String,
        pub file_size: usize,
    }

    #[derive(Debug, Facet)]
    pub struct UploadDoneMessage {
        pub uploaded_size: usize,
    }

    #[derive(Debug, Facet)]
    pub struct TranscodingCompleteMessage {
        pub output_size: usize,
    }

    #[derive(Debug, Clone, Facet)]
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

    #[derive(Debug, Clone, Facet)]
    #[repr(u8)]
    pub enum TranscodeEvent {
        MediaIdentified(MediaProps),
        Progress(TranscodingProgress),
    }
}

#[derive(Debug, Clone, Facet)]
pub struct ListMissingArgs {
    /// queries if ObjectStoreKey is in object storage, if
    /// not we'll return the InputPath
    pub objects_to_query: HashMap<ObjectStoreKey, InputPath>,

    /// this is only set when a local mom is hitting
    /// the production mom — to make sure the local
    /// info becomes remote, too.
    pub mark_these_as_uploaded: Option<Vec<ObjectStoreKey>>,
}

#[derive(Debug, Clone, Facet)]
pub struct ListMissingResponse {
    pub missing: HashMap<ObjectStoreKey, InputPath>,
}

#[derive(Debug, Facet)]
#[repr(u8)]
pub enum MomEvent {
    GoodMorning(GoodMorning),
    TenantEvent(TenantEvent),
}

#[derive(Debug, Facet)]
pub struct TenantEvent {
    pub tenant_name: TenantDomain,
    pub payload: TenantEventPayload,
}

#[derive(Facet, Clone, Default)]
pub struct AllUsers {
    pub users: HashMap<UserId, UserInfo>,
}

#[derive(Facet)]
#[repr(u8)]
pub enum TenantEventPayload {
    RevisionChanged(Box<Pak>),
    /// cubs derive sponsors to show from user info themselves. there's no delta
    /// going on here — we just send them all user info every few seconds.
    UsersUpdated(AllUsers),
}

impl std::fmt::Debug for TenantEventPayload {
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
            TenantEventPayload::UsersUpdated(users) => {
                write!(f, "TenantEvent::UsersUpdated({} users)", users.users.len())
            }
        }
    }
}

#[derive(Debug, Facet)]
pub struct GoodMorning {
    pub initial_states: HashMap<TenantDomain, TenantInitialState>,
}

#[derive(Facet)]
pub struct TenantInitialState {
    /// The revision to serve (if any)
    pub pak: Option<Pak>,

    /// The users for this tenant
    pub users: Option<AllUsers>,

    /// The configuration for this tenant
    pub tc: TenantConfig,

    /// if mom and cub are colocated, they can share a data dir (especially important in dev)
    pub base_dir: Option<Utf8PathBuf>,
}

impl std::fmt::Debug for TenantInitialState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TenantInitialState").finish_non_exhaustive()
    }
}

pub struct MomServeArgs {
    pub config: MomConfig,
    pub web: WebConfig,
    pub tenants: HashMap<TenantDomain, TenantInfo>,
    pub listener: tokio::net::TcpListener,
}

/// Returns a 64-character hex string that's deterministic and unique per tenant
/// Uses HMAC to be secure even if tenant names become user-controlled in the future
pub fn derive_cookie_sauce(global_sauce: &str, tenant_name: &TenantDomain) -> String {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    type HmacSha256 = Hmac<Sha256>;

    let mut mac =
        HmacSha256::new_from_slice(global_sauce.as_bytes()).expect("HMAC can take key of any size");
    mac.update(tenant_name.as_str().as_bytes());
    let result = mac.finalize();
    hex::encode(result.into_bytes())
}

#[derive(Debug, Clone, Facet)]
pub struct PatreonCallbackResponse {
    pub user_info: UserInfo,
}

#[derive(Debug, Clone, Facet)]
pub struct GithubCallbackResponse {
    pub user_info: UserInfo,
}
