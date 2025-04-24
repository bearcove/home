use autotrait::autotrait;
use futures_core::future::BoxFuture;
// TODO: move me to `image-types` or something to avoid rebuilds
use image_types::IntrinsicPixels;
use std::sync::{Arc, LazyLock};

#[derive(Debug, Clone)]
pub struct Image {
    pub url: String,
    pub width: Option<IntrinsicPixels>,
    pub height: Option<IntrinsicPixels>,
    pub data_url: String,
}

#[derive(Debug, Clone)]
pub struct WebpageInfo {
    pub title: Option<String>,
    pub description: Option<String>,
    pub url: String, // canonical URL
    pub image: Option<Image>,
}

merde::derive! {
    impl (Serialize) for struct WebpageInfo { title, description, url, image }
}

merde::derive! {
    impl (Serialize) for struct Image { url, width, height, data_url }
}

struct ModImpl {
    client: Arc<dyn libhttpclient::HttpClient>,
}

impl Default for ModImpl {
    fn default() -> Self {
        Self {
            client: libhttpclient::load().client().into(),
        }
    }
}

static MOD: LazyLock<ModImpl> = LazyLock::new(ModImpl::default);

pub fn load() -> &'static dyn Mod {
    &*MOD
}

#[autotrait]
impl Mod for ModImpl {
    fn webpage_info(&self, url: String) -> BoxFuture<'static, Result<WebpageInfo, String>> {
        let client = self.client.clone();
        Box::pin(impls::get_webpage_info(client, url))
    }
}

mod impls;
