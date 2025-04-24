use autotrait::autotrait;
pub use bytes::Bytes;

use futures_core::future::{BoxFuture, LocalBoxFuture};
use futures_util::stream::BoxStream;
use objectstore_types::{ObjectStoreKey, ObjectStoreKeyRef};
use std::{borrow::Cow, ops::Range, sync::Arc};

use config_types::{AwsSecrets, Environment, ObjectStorageConfig};

use futures_util::stream::StreamExt;
use object_store::aws::AmazonS3Builder;
use object_store::path::Path;
use std::fmt;

/// Options for a put request
#[derive(Default, Clone)]
pub struct PutOptions {
    /// Content type of the object
    pub content_type: Option<Cow<'static, str>>,
}

pub struct PutResult {
    pub e_tag: Option<String>,
    pub version: Option<String>,
}

/// Options for a put_multipart request
pub type PutMultipartOpts = PutOptions;

#[derive(Clone)]
pub enum GetRange {
    /// Request a specific range of bytes
    ///
    /// If the given range is zero-length or starts after the end of the object,
    /// an error will be returned. Additionally, if the range ends after the end
    /// of the object, the entire remainder of the object will be returned.
    /// Otherwise, the exact requested range will be returned.
    Bounded(Range<usize>),
    /// Request all bytes starting from a given byte offset
    Offset(usize),
    /// Request up to the last n bytes
    Suffix(usize),
}

#[derive(Clone, Default)]
pub struct GetOptions {
    /// Byte range to request
    pub range: Option<GetRange>,
    /// Whether this is a HEAD request
    pub head: bool,
}

#[derive(Debug)]
pub enum ErrorKind {
    NotFound,
    Other,
}

impl std::fmt::Display for ErrorKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl ErrorKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ErrorKind::NotFound => "not found",
            ErrorKind::Other => "other",
        }
    }
}

#[derive(Debug)]
pub struct Error {
    pub kind: ErrorKind,
    pub source: Box<dyn std::error::Error + Send + Sync>,
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.kind)
    }
}

impl std::error::Error for Error {}

impl Error {
    pub fn is_not_found(&self) -> bool {
        matches!(self.kind, ErrorKind::NotFound)
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;

pub struct LayeredBuilder<'a, M: ?Sized> {
    m: &'a M,
    stores: Vec<(String, Arc<dyn ObjectStore>)>,
}

impl<'a, M: Mod + ?Sized> LayeredBuilder<'a, M> {
    pub fn new(m: &'a M) -> Self {
        Self {
            m,
            stores: Default::default(),
        }
    }

    /// Add a store to the layered store
    pub fn layer(mut self, name: String, store: Arc<dyn ObjectStore>) -> Self {
        self.stores.push((name, store));
        self
    }

    /// Build the layered store
    pub fn finish(self) -> Arc<dyn ObjectStore> {
        self.m.layered(self.stores)
    }
}

struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
    fn s3(
        &self,
        config: &ObjectStorageConfig,
        secrets: &AwsSecrets,
    ) -> Result<Arc<dyn ObjectStore>> {
        let mut s3_builder = AmazonS3Builder::new()
            .with_region(config.region.as_str())
            .with_bucket_name(config.bucket.as_str())
            .with_access_key_id(&secrets.access_key_id)
            .with_secret_access_key(&secrets.secret_access_key);

        if let Some(endpoint) = &config.endpoint {
            s3_builder = s3_builder.with_endpoint(endpoint.as_str());
        }

        let s3 = s3_builder.build().map_err(to_spec_error)?;
        Ok(Arc::new(ObjectStoreWrapper {
            desc: format!("S3 (region: {}, bucket: {})", config.region, config.bucket),
            inner: Box::new(s3),
        }))
    }

    fn local_disk_with_prefix(&self, prefix: &str) -> Result<Arc<dyn ObjectStore>> {
        Ok(Arc::new(ObjectStoreWrapper {
            desc: format!("Local disk (prefix: {prefix})"),
            inner: Box::new(
                object_store::local::LocalFileSystem::new_with_prefix(prefix)
                    .map_err(to_spec_error)?,
            ),
        }))
    }

    fn in_memory(&self) -> Arc<dyn ObjectStore> {
        Arc::new(ObjectStoreWrapper {
            desc: "In-memory".to_string(),
            inner: Box::new(object_store::memory::InMemory::new()),
        })
    }

    fn layered(&self, stores: Vec<(String, Arc<dyn ObjectStore>)>) -> Arc<dyn ObjectStore> {
        Arc::new(LayeredStore {
            stores: stores
                .into_iter()
                .map(|(name, store)| Layer { name, store })
                .collect(),
        })
    }
}

fn to_spec_error(e: object_store::Error) -> Error {
    Error {
        kind: match &e {
            object_store::Error::NotFound { .. } => ErrorKind::NotFound,
            _ => ErrorKind::Other,
        },
        source: Box::new(e),
    }
}

fn from_spec_put_opts(opts: PutOptions) -> object_store::PutOptions {
    let mut out = object_store::PutOptions::default();
    if let Some(content_type) = opts.content_type {
        out.attributes.insert(
            object_store::Attribute::ContentType,
            content_type.into_owned().into(),
        );
    }
    out
}

fn from_spec_put_multipart_opts(opts: PutMultipartOpts) -> object_store::PutMultipartOpts {
    let mut out = object_store::PutMultipartOpts::default();
    if let Some(content_type) = opts.content_type {
        out.attributes.insert(
            object_store::Attribute::ContentType,
            content_type.into_owned().into(),
        );
    }
    out
}

fn from_spec_getrange(range: GetRange) -> object_store::GetRange {
    match range {
        GetRange::Bounded(range) => object_store::GetRange::Bounded(range.start..range.end),
        GetRange::Offset(offset) => object_store::GetRange::Offset(offset),
        GetRange::Suffix(suffix) => object_store::GetRange::Suffix(suffix),
    }
}

fn from_spec_get_options(opts: GetOptions) -> object_store::GetOptions {
    let mut out = object_store::GetOptions::default();
    if let Some(range) = opts.range {
        out.range = Some(from_spec_getrange(range));
    }
    out.head = opts.head;
    out
}

fn to_spec_put_result(res: object_store::PutResult) -> PutResult {
    PutResult {
        e_tag: res.e_tag,
        version: res.version,
    }
}

struct ObjectStoreWrapper {
    desc: String,
    inner: Box<dyn object_store::ObjectStore + Send + Sync>,
}

#[autotrait]
impl ObjectStore for ObjectStoreWrapper {
    fn put_opts(
        &self,
        key: &ObjectStoreKeyRef,
        payload: Bytes,
        opts: PutOptions,
    ) -> BoxFuture<'_, Result<PutResult>> {
        let path = Path::from(key.as_str());
        Box::pin(async move {
            Ok(to_spec_put_result(
                self.inner
                    .put_opts(&path, payload.into(), from_spec_put_opts(opts))
                    .await
                    .map_err(to_spec_error)?,
            ))
        })
    }

    fn put_multipart_opts(
        &self,
        key: &ObjectStoreKeyRef,
        payload: PutMultipartOpts,
    ) -> BoxFuture<'_, Result<Box<dyn MultipartUpload>>> {
        let path = Path::from(key.as_str());
        Box::pin(async move {
            let inner_upload = self
                .inner
                .put_multipart_opts(&path, from_spec_put_multipart_opts(payload))
                .await
                .map_err(to_spec_error)?;
            Ok(Box::new(MultipartUploadWrapper(inner_upload)) as Box<dyn MultipartUpload>)
        })
    }

    fn get_opts(
        &self,
        key: &ObjectStoreKeyRef,
        opts: GetOptions,
    ) -> BoxFuture<'_, Result<Box<dyn GetResult>>> {
        let path = Path::from(key.as_str());
        Box::pin(async move {
            let res = self
                .inner
                .get_opts(&path, from_spec_get_options(opts))
                .await
                .map_err(to_spec_error)?;
            let get_result: Box<dyn GetResult> = Box::new(GetResultWrapper(res));
            Ok(get_result)
        })
    }

    fn desc(&self) -> String {
        self.desc.clone()
    }
}

impl dyn ObjectStore {
    pub fn get(&self, key: &ObjectStoreKeyRef) -> BoxFuture<'_, Result<Box<dyn GetResult>>> {
        self.get_opts(key, GetOptions::default())
    }

    pub fn put(&self, key: &ObjectStoreKeyRef, payload: Bytes) -> BoxFuture<'_, Result<PutResult>> {
        self.put_opts(key, payload, PutOptions::default())
    }
}

struct MultipartUploadWrapper(Box<dyn object_store::MultipartUpload>);

#[autotrait(!Send)]
impl MultipartUpload for MultipartUploadWrapper {
    fn put_part(&mut self, data: Bytes) -> LocalBoxFuture<'static, Result<()>> {
        let fut = self.0.put_part(data.into());
        Box::pin(async move {
            fut.await.map_err(to_spec_error)?;
            Ok(())
        })
    }

    fn complete(mut self: Box<Self>) -> LocalBoxFuture<'static, Result<PutResult>> {
        Box::pin(async move {
            let result = self.0.complete().await.map_err(to_spec_error)?;
            Ok(to_spec_put_result(result))
        })
    }
}

struct Layer {
    name: String,
    store: Arc<dyn ObjectStore>,
}

pub(crate) struct LayeredStore {
    stores: Vec<Layer>,
}

impl fmt::Debug for LayeredStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LayeredStore")
            .field(
                "stores",
                &self
                    .stores
                    .iter()
                    .map(|layer| &layer.name)
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl fmt::Display for LayeredStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl ObjectStore for LayeredStore {
    fn put_opts(
        &self,
        key: &ObjectStoreKeyRef,
        payload: Bytes,
        opts: PutOptions,
    ) -> BoxFuture<'_, Result<PutResult>> {
        let key = key.to_owned();

        Box::pin(async move {
            let futures = self.stores.iter().map(|layer| {
                let key = key.clone();
                let payload = payload.clone();
                let opts = opts.clone();
                let store = Arc::clone(&layer.store);
                let name = layer.name.clone();

                tokio::spawn(async move {
                    match store.put_opts(&key, payload, opts).await {
                        Ok(result) => {
                            tracing::debug!(
                                "Successfully put \x1b[32m{}\x1b[0m in \x1b[32m{}\x1b[0m",
                                key,
                                name
                            );
                            Ok(result)
                        }
                        Err(e) => {
                            tracing::warn!(
                                "Failed to put object in store: {}, error: {:?}",
                                name,
                                e
                            );
                            Err(e)
                        }
                    }
                })
            });

            futures_util::future::try_join_all(futures)
                .await
                .unwrap()
                .into_iter()
                .collect::<Result<Vec<_>, _>>()?;
            Ok(PutResult {
                e_tag: None,
                version: None,
            })
        })
    }

    fn get_opts(
        &self,
        key: &ObjectStoreKeyRef,
        opts: GetOptions,
    ) -> BoxFuture<'_, Result<Box<dyn GetResult>>> {
        tracing::trace!(%key, "layered store get");
        let key = key.to_owned();

        Box::pin(async move {
            if opts.range.is_some() {
                return Err(Error {
                    kind: ErrorKind::Other,
                    source: Box::new(std::io::Error::new(
                        std::io::ErrorKind::Unsupported,
                        "Range requests are not supported",
                    )),
                });
            }

            let mut found: Option<(usize, Box<dyn GetResult>)> = None;

            for (index, layer) in self.stores.iter().enumerate() {
                tracing::trace!(%key, %index, desc = layer.store.desc(), "trying layer");

                match layer.store.get_opts(&key, opts.clone()).await {
                    Ok(result) => {
                        tracing::debug!(
                            "found object \x1b[32m{}\x1b[0m in \x1b[32m{}\x1b[0m",
                            key,
                            layer.name
                        );
                        found = Some((index, result));
                        break;
                    }
                    Err(e) => {
                        tracing::trace!(%key, %index, desc = layer.store.desc(), ?e, "layer error");
                        if matches!(e.kind, ErrorKind::NotFound) {
                            continue;
                        }
                        return Err(e);
                    }
                }
            }

            match found {
                Some((found_index, res)) => {
                    // collect info about the found object
                    let size = res.size();
                    let content_type = res.content_type().map(|s| s.to_owned());

                    // collect the bytes
                    let bytes = res.bytes().await?;

                    // Insert into higher layers
                    for layer in self.stores.iter().take(found_index) {
                        let key = key.to_owned();
                        let payload = bytes.clone();
                        let opts = PutOptions::default();
                        let store = Arc::clone(&layer.store);
                        let name = layer.name.clone();

                        match store.put_opts(&key, payload, opts).await {
                            Ok(_) => {
                                tracing::debug!(
                                    "Successfully inserted object in higher layer: {}",
                                    name
                                )
                            }
                            Err(e) => tracing::warn!(
                                "Failed to insert object in higher layer: {}, error: {:?}",
                                name,
                                e
                            ),
                        }
                    }

                    // make a new result
                    let result = SimpleGetResult {
                        size,
                        content_type,
                        bytes,
                    };
                    let result: Box<dyn GetResult> = Box::new(result);
                    Ok(result)
                }
                _ => Err(Error {
                    kind: ErrorKind::NotFound,
                    source: Box::new(LayeredNotFound),
                }),
            }
        })
    }

    fn put_multipart_opts(
        &self,
        _key: &ObjectStoreKeyRef,
        _payload: PutMultipartOpts,
    ) -> BoxFuture<'_, Result<Box<dyn MultipartUpload>>> {
        Box::pin(async move {
            Err(Error {
                kind: ErrorKind::Other,
                source: Box::new(std::io::Error::new(
                    std::io::ErrorKind::Unsupported,
                    "Multipart upload not implemented for LayeredStore",
                )),
            })
        })
    }

    fn desc(&self) -> String {
        format!(
            "LayeredStore({})",
            self.stores
                .iter()
                .map(|layer| layer.store.desc())
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

#[derive(Debug)]
struct LayeredNotFound;

impl std::error::Error for LayeredNotFound {}

impl fmt::Display for LayeredNotFound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Object not found in any layer of the LayeredStore")
    }
}

struct GetResultWrapper(object_store::GetResult);

#[autotrait(!Sync)]
impl GetResult for GetResultWrapper {
    fn size(&self) -> usize {
        self.0.meta.size
    }

    fn range(&self) -> std::ops::Range<usize> {
        self.0.range.clone()
    }

    fn content_type(&self) -> Option<&str> {
        self.0
            .attributes
            .get(&object_store::Attribute::ContentType)
            .map(|v| -> &str { v })
    }

    fn bytes(self: Box<Self>) -> BoxFuture<'static, Result<Bytes>> {
        Box::pin(async move { self.0.bytes().await.map_err(to_spec_error) })
    }

    fn into_stream(self: Box<Self>) -> BoxStream<'static, Result<Bytes>> {
        Box::pin(
            self.0
                .into_stream()
                .map(|result| result.map_err(to_spec_error)),
        )
    }
}

struct SimpleGetResult {
    size: usize,
    content_type: Option<String>,
    bytes: Bytes,
}

impl GetResult for SimpleGetResult {
    fn size(&self) -> usize {
        self.size
    }

    fn range(&self) -> std::ops::Range<usize> {
        0..self.size
    }

    fn content_type(&self) -> Option<&str> {
        self.content_type.as_deref()
    }

    fn bytes(self: Box<Self>) -> BoxFuture<'static, Result<Bytes>> {
        Box::pin(async move { Ok(self.bytes) })
    }

    fn into_stream(self: Box<Self>) -> BoxStream<'static, Result<Bytes>> {
        Box::pin(futures_util::stream::once(async move { Ok(self.bytes) }))
    }
}

pub fn input_key(hash: &str, ext: &str) -> ObjectStoreKey {
    let prefix = "inputs";
    let first_two = &hash[..2];
    if ext.is_empty() {
        ObjectStoreKey::new(format!("{prefix}/{first_two}/{hash}"))
    } else {
        ObjectStoreKey::new(format!("{prefix}/{first_two}/{hash}.{ext}"))
    }
}

pub fn derivation_key(env: Environment, hash: &str, ext: &str) -> ObjectStoreKey {
    let prefix = if env.is_prod() {
        "production/derivations"
    } else {
        "development/derivations"
    };
    let first_two = &hash[..2];
    if ext.is_empty() {
        ObjectStoreKey::new(format!("{prefix}/{first_two}/{hash}"))
    } else {
        ObjectStoreKey::new(format!("{prefix}/{first_two}/{hash}.{ext}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn object_store_is_object_safe() {
        fn assert_object_safe<T: ?Sized>() {}
        assert_object_safe::<dyn ObjectStore>();

        let _: Box<dyn ObjectStore>;
    }
}
