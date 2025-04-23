use std::{sync::Arc, time::Duration};

use base64::Engine as _;
use futures_util::StreamExt;
use image::ICodec;
use libhttpclient::{
    Method, Response,
    header::{HeaderName, HeaderValue},
};
use tokio::time::timeout;
use webpage::HTML;

use crate::{Image, WebpageInfo};

pub(crate) async fn get_webpage_info(
    client: Arc<dyn libhttpclient::HttpClient>,
    url: String,
) -> Result<WebpageInfo, String> {
    let url = url::Url::parse(&url).map_err(|e| e.to_string())?;
    let base_url = format!("{}://{}", url.scheme(), url.host_str().unwrap_or(""));

    let uri = libhttpclient::Uri::try_from(url.as_str()).map_err(|e| e.to_string())?;
    use libhttpclient::header;

    let request = client
        .request(Method::GET, uri)
        .browser_like_user_agent()
        .header(
            header::ACCEPT,
            HeaderValue::from_static(
                "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
            ),
        )
        .header(
            HeaderName::from_static("accept-language"),
            HeaderValue::from_static("en-US,en;q=0.9"),
        )
        .header(
            HeaderName::from_static("sec-fetch-dest"),
            HeaderValue::from_static("document"),
        )
        .header(
            HeaderName::from_static("sec-fetch-mode"),
            HeaderValue::from_static("navigate"),
        )
        .header(
            HeaderName::from_static("sec-fetch-site"),
            HeaderValue::from_static("none"),
        );

    let response = timeout(Duration::from_secs(10), request.send())
        .await
        .map_err(|_| "Request timed out".to_string())?
        .map_err(|e| e.to_string())?;

    let data = response.text().await.map_err(|e| e.to_string())?;
    let html = HTML::from_string(data, Some(url.as_str().to_owned())).map_err(|e| e.to_string())?;

    let canonical_url = html.url.clone().unwrap_or_else(|| url.as_str().to_owned());
    let canonical_url = if canonical_url.starts_with('/') {
        format!("{base_url}{canonical_url}")
    } else {
        canonical_url
    };
    eprintln!("Canonical URL: {canonical_url}");

    let mut info = WebpageInfo {
        title: html.title.clone(),
        description: html.description.clone(),
        url: canonical_url.clone(),
        image: None,
    };
    add_image_maybe(&mut info, &html, client.as_ref()).await;

    Ok(info)
}

pub(crate) async fn add_image_maybe(
    info: &mut WebpageInfo,
    html: &HTML,
    client: &dyn libhttpclient::HttpClient,
) {
    let img = match html.opengraph.images.first() {
        Some(img) => img,
        None => return,
    };
    let url = if img.url.starts_with('/') {
        if let Ok(base_url) = url::Url::parse(&info.url) {
            format!(
                "{}://{}{}",
                base_url.scheme(),
                base_url.host_str().unwrap_or(""),
                img.url
            )
        } else {
            img.url.clone()
        }
    } else {
        img.url.clone()
    };

    let res = match client
        .request(Method::GET, url.parse::<libhttpclient::Uri>().unwrap())
        .browser_like_user_agent()
        .send()
        .await
        .map_err(|e| e.to_string())
    {
        Ok(res) => res,
        Err(e) => {
            eprintln!("Failed to fetch image: {e}");
            return;
        }
    };

    let headers = res.headers_only_string_safe();

    let content_type = headers
        .get("content-type")
        .map(|v| v.as_str())
        .unwrap_or_default();

    if content_type.starts_with("image/svg+xml") {
        let bytes = match get_limited_body(res).await {
            Ok(body) => body,
            Err(e) => {
                eprintln!("Error fetching limited body: {}", e);
                return;
            }
        };
        let base64_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        info.image = Some(Image {
            url,
            width: None,
            height: None,
            data_url: format!("data:image/svg+xml;base64,{base64_data}"),
        });
        return;
    }

    let iformat = match ICodec::from_content_type_str(content_type) {
        Some(iformat) => iformat,
        None => {
            eprintln!("Unsupported image content type: {content_type:?} (from {url})");
            return;
        }
    };

    let bytes = match get_limited_body(res).await {
        Ok(body) => body,
        Err(e) => {
            eprintln!("Error fetching limited body: {}", e);
            return;
        }
    };

    let mod_img = image::load();
    let (width, height) = match mod_img.dimensions(&bytes, iformat) {
        Ok(dims) => dims,
        Err(e) => {
            eprintln!("Error while fetching image dimensions: {}", e);
            return;
        }
    };

    let transcoded = match mod_img.transcode(&bytes, iformat, ICodec::WEBP, None) {
        Ok(transcoded) => transcoded,
        Err(e) => {
            eprintln!("Error while transcoding image: {}", e);
            return;
        }
    };
    let base64_data = base64::engine::general_purpose::STANDARD.encode(&transcoded);
    let data_url_type = "image/webp";
    info.image = Some(Image {
        url,
        width: Some(width),
        height: Some(height),
        data_url: format!("data:{data_url_type};base64,{base64_data}"),
    });
}

async fn get_limited_body(res: Box<dyn Response>) -> Result<Vec<u8>, String> {
    let mut total_size: usize = 0;
    let mut bytes = Vec::new();

    let mut stream = res.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = match chunk {
            Ok(chunk) => chunk,
            Err(e) => {
                eprintln!("Error while fetching image chunk: {}", e);
                return Err("Error fetching image chunk".to_string());
            }
        };
        total_size += chunk.len();
        const LIMIT: usize = 8 * 1024 * 1024; // 8MB
        if total_size > LIMIT {
            eprintln!("Image size exceeds 8MB limit");
            return Err("Image size exceeds limit".to_string());
        }
        bytes.extend_from_slice(&chunk);
    }

    Ok(bytes)
}
