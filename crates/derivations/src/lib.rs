use std::{borrow::Cow, sync::Arc};

use closest::GetOrHelp;
use config::{Environment, TenantInfo, WebConfig};
use conflux::{
    ACodec, BS, BsForResults, Derivation, DerivationBitmap, DerivationHash, DerivationKind,
    DerivationVideo, DerivationVideoThumbnail, Input, Pak, PathMappings, PipelineHashRef, Route,
    VCodec, VContainer,
};
use content_type::ContentType;
use image::ICodec;
use objectstore::{Bytes, LayeredBuilder, ObjectStore, ObjectStoreKey, derivation_key};

#[derive(Debug)]
pub struct DerivationInfo<'a> {
    pub input: &'a Input,
    pub derivation: &'a Derivation,
}

impl<'a> DerivationInfo<'a> {
    pub fn lookup(
        pak: &'a Pak<'static>,
        derivation: &'a Derivation,
    ) -> eyre::Result<DerivationInfo<'a>> {
        let input = pak
            .inputs
            .get_or_help(closest::ResourceKind::Input, &derivation.input)
            .bs()?;
        Ok(DerivationInfo { input, derivation })
    }

    pub fn new(input: &'a Input, derivation: &'a Derivation) -> Self {
        DerivationInfo { input, derivation }
    }

    pub async fn fetch_original(
        &self,
        ti: Arc<TenantInfo>,
        object_store: Arc<dyn ObjectStore>,
        web: WebConfig,
    ) -> eyre::Result<Bytes> {
        let e = match object_store.get(&self.input.key()).await {
            Ok(res) => return res.bytes().await.bs(),
            Err(e) => e,
        };
        if e.is_not_found() && web.env.is_dev() {
            // in development, inputs are sometimes only on disk
            let mappings = PathMappings::from_ti(ti.as_ref());
            match mappings.to_disk_path(&self.input.path) {
                Ok(disk_path) => match tokio::fs::read(&disk_path).await {
                    Ok(bytes) => return Ok(bytes.into()),
                    Err(read_err) => {
                        return Err(BS::from_string(format!(
                            "Failed to fetch original: not found in object store and also not found on disk at {disk_path}. Error: {read_err}"
                        )));
                    }
                },
                Err(path_err) => {
                    return Err(BS::from_string(format!(
                        "Failed to fetch original: not found in object store and couldn't map to disk path. Error: {path_err}"
                    )));
                }
            }
        }

        Err(BS::from_string(format!(
            "Failed to fetch original for key '{}': {}",
            self.input.key(),
            e
        )))
    }
}

impl DerivationInfo<'_> {
    pub fn content_type(&self) -> ContentType {
        match self.derivation.kind {
            DerivationKind::Passthrough(_) => self.input.content_type,
            DerivationKind::Identity(_) => self.input.content_type,
            DerivationKind::Bitmap(DerivationBitmap { ic, .. }) => ic.content_type(),
            DerivationKind::Video(DerivationVideo { container, .. }) => container.content_type(),
            DerivationKind::VideoThumbnail(DerivationVideoThumbnail { ic }) => ic.content_type(),
            DerivationKind::DrawioRender(_) => ContentType::SVG,
            DerivationKind::SvgCleanup(_) => ContentType::SVG,
        }
    }

    /// The file extension we should use, without leading dot.
    /// Sometimes it can return `png` or `jxl`, but sometimes it can return `@2x.png`
    /// for example.
    fn ext(&self) -> Cow<'_, str> {
        match &self.derivation.kind {
            DerivationKind::Passthrough(_) => {
                let (_, ext) = self.input.path.explode();
                ext.into()
            }
            DerivationKind::Identity(_) => {
                let (_, ext) = self.input.path.explode();
                ext.into()
            }
            DerivationKind::Bitmap(derivation_bitmap) => {
                // example outputs for bitmaps:
                // file.png       (original width)
                // file.w400.png  (400px width)
                let mut s = String::new();
                if let Some(width) = derivation_bitmap.width {
                    s.push('w');
                    s.push_str(&width.to_string());
                    s.push('.');
                }
                s.push_str(derivation_bitmap.ic.content_type().ext());
                s.into()
            }
            DerivationKind::Video(derivation_video) => {
                let vc = derivation_video.vc.to_string();
                let ac = derivation_video.ac.to_string();
                let container_ext = derivation_video.container.content_type().ext();
                format!("{}+{}.{}", vc, ac, container_ext).into()
            }
            DerivationKind::VideoThumbnail(derivation_video_thumbnail) => {
                derivation_video_thumbnail.ic.ext().into()
            }
            DerivationKind::DrawioRender(_) => "svg".into(),
            DerivationKind::SvgCleanup(_) => "svg".into(),
        }
    }

    /// Where the output should be stored in the tenant's object store
    pub fn key(&self, env: config::Environment) -> ObjectStoreKey {
        derivation_key(env, self.hash().as_str(), self.content_type().ext())
    }

    /// The route the derivation will be served from.
    pub fn route(&self) -> Route {
        if let DerivationKind::Passthrough(_) = self.derivation.kind {
            // passthrough derivations are not cache-busted (they were already
            // busted by vite (a javascript bundler), for example)
            Route::new(self.input.path.to_string())
        } else {
            let (base, _ext) = self.input.path.explode();
            let hash = self.hash();
            let ext = self.ext();
            // note: input paths contain an initial `/` so we don't need to add it here.
            Route::new(format!("{base}~{hash}.{ext}"))
        }
    }

    /// The hash of the derivation
    pub fn hash(&self) -> DerivationHash {
        let mut mixer = HashMixer::new();
        mixer.mix(self.input.hash.as_ref());

        match &self.derivation.kind {
            DerivationKind::Passthrough(_) | DerivationKind::Identity(_) => {
                // for these, we simply return the input hash directly
                return DerivationHash::new(self.input.hash.to_string());
            }
            DerivationKind::Bitmap(d) => {
                d.ic.add_pipeline_hash(&mut mixer);
                // mix target width, if any
                if let Some(w) = d.width {
                    mixer.mix(&format!("w{w}"));
                }
            }
            DerivationKind::Video(d) => {
                d.vc.add_pipeline_hash(&mut mixer);
                d.container.add_pipeline_hash(&mut mixer);
            }
            DerivationKind::VideoThumbnail(d) => {
                d.ic.add_pipeline_hash(&mut mixer);
                mixer.mix(VIDEO_THUMB_PIPELINE_HASH.as_str());
            }
            DerivationKind::DrawioRender(_) => {
                mixer.mix(DRAWIO_PIPELINE_HASH.as_str());
            }
            DerivationKind::SvgCleanup(_) => {
                mixer.mix(SVG_CLEANUP_PIPELINE_HASH.as_str());
            }
        }
        mixer.finish()
    }
}

#[derive(Default)]
pub struct HashMixer {
    buf: Vec<u8>,
}

impl HashMixer {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn mix(&mut self, s: &str) {
        self.buf.extend_from_slice(s.as_bytes());
    }

    pub fn finish(self) -> DerivationHash {
        let h = seahash::hash(&self.buf[..]);
        DerivationHash::new(format!("{h:016x}"))
    }
}

pub trait HasPipelineHash {
    fn add_pipeline_hash(&self, mixer: &mut HashMixer);
}

impl HasPipelineHash for ICodec {
    fn add_pipeline_hash(&self, mixer: &mut HashMixer) {
        mixer.mix(match self {
            ICodec::JXL => "jxl-pipeline-2025-01-30",
            ICodec::AVIF => "avif-pipeline-2025-01-30",
            ICodec::WEBP => "webp-pipeline-2025-01-30",
            ICodec::PNG => "png-pipeline-2024-01-28",
            ICodec::JPG => "jpg-pipeline-2024-01-28",
            ICodec::HEIC => "heic-pipeline-2025-02-05",
        });
    }
}

impl HasPipelineHash for VCodec {
    fn add_pipeline_hash(&self, mixer: &mut HashMixer) {
        mixer.mix(match self {
            VCodec::AV1 => "av1-pipeline-2025-01-26",
            VCodec::VP9 => "vp9-pipeline-2025-01-26",
            VCodec::AVC => "avc-pipeline-2025-01-26",
        });
    }
}

impl HasPipelineHash for ACodec {
    fn add_pipeline_hash(&self, mixer: &mut HashMixer) {
        mixer.mix(match self {
            ACodec::Opus => "opus-pipeline-2025-01-26",
            ACodec::Aac => "aac-pipeline-2025-01-26",
        });
    }
}

const DRAWIO_PIPELINE_HASH: &PipelineHashRef =
    PipelineHashRef::from_static("drawio-pipeline-2025-03-25b");
const SVG_CLEANUP_PIPELINE_HASH: &PipelineHashRef =
    PipelineHashRef::from_static("svg-cleanup-pipeline-2025-02-24");
const VIDEO_THUMB_PIPELINE_HASH: &PipelineHashRef =
    PipelineHashRef::from_static("video-thumb-pipeline-2025-01-30b");

impl HasPipelineHash for VContainer {
    fn add_pipeline_hash(&self, mixer: &mut HashMixer) {
        mixer.mix(match self {
            VContainer::MP4 => "mp4-container-2025-01-26",
            VContainer::WebM => "webm-container-2025-01-26",
        });
    }
}

pub async fn objectstore_for_tenant(
    ti: &TenantInfo,
    env: Environment,
) -> eyre::Result<Arc<dyn ObjectStore>> {
    let objectstore = objectstore::load();

    let disk_path = ti.internal_dir().join("object-cache");
    if !disk_path.exists() {
        tokio::fs::create_dir_all(&disk_path).await.unwrap();
    }

    let mut builder = LayeredBuilder::new(objectstore)
        .layer("memory".to_string(), objectstore.in_memory())
        .layer(
            "disk".to_string(),
            objectstore
                .local_disk_with_prefix(disk_path.as_str())
                .unwrap(),
        );

    if env.is_prod() {
        let object_storage = ti
            .tc
            .object_storage
            .as_ref()
            .expect("object_storage must be set in production");
        let secrets = ti
            .tc
            .secrets
            .as_ref()
            .expect("secrets must be set in production");
        builder = builder.layer(
            "s3".to_string(),
            objectstore.s3(object_storage, &secrets.aws).unwrap(),
        )
    }

    Ok(builder.finish())
}
