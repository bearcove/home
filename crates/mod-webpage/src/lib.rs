#[cfg(feature = "impl")]
use std::sync::Arc;

use futures_core::future::BoxFuture;
use image::IntrinsicPixels;

include!(".dylo/spec.rs");
include!(".dylo/support.rs");

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

#[cfg(feature = "impl")]
struct ModImpl {
    client: Arc<dyn libhttpclient::HttpClient>,
}

#[cfg(feature = "impl")]
impl Default for ModImpl {
    fn default() -> Self {
        Self {
            client: libhttpclient::load().client().into(),
        }
    }
}

#[dylo::export]
impl Mod for ModImpl {
    fn webpage_info(&self, url: String) -> BoxFuture<'static, Result<WebpageInfo, String>> {
        let client = self.client.clone();
        Box::pin(impls::get_webpage_info(client, url))
    }
}

#[cfg(feature = "impl")]
mod impls;
