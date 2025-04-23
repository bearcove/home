use cub_types::CubReq;
use hattip::prelude::*;
use url::Url;

pub(crate) async fn serve_comments(rcx: Box<dyn CubReq>) -> HReply {
    let params = rcx.url_params_map();
    let url = params
        .get("url")
        .ok_or_else(|| HError::bad_request("Missing 'url' parameter"))?;
    let title = params
        .get("title")
        .ok_or_else(|| HError::bad_request("Missing 'title' parameter"))?;

    let submit_url = {
        let mut u = Url::parse("https://reddit.com/r/fasterthanlime/submit").unwrap();
        let mut q = u.query_pairs_mut();
        q.append_pair("url", url);
        q.append_pair("title", title);
        drop(q);
        u
    }
    .to_string();

    let reddit_secrets = rcx.reddit_secrets().map_err(|_| HError::WithStatus {
        status_code: StatusCode::INTERNAL_SERVER_ERROR,
        msg: "Failed to get reddit secrets".into(),
    })?;
    let submission_url_res = reddit::load().get_submission(reddit_secrets, url).await;
    let redirect_url = match submission_url_res {
        Err(e) => {
            tracing::warn!("Reddit API error: {}", e);
            submit_url
        }
        Ok(maybe) => maybe.unwrap_or(submit_url),
    };

    Response::builder()
        .status(StatusCode::SEE_OTHER)
        .header(header::LOCATION, &redirect_url)
        .body(HBody::empty())
        .into_reply()
}
