use autotrait::autotrait;
use eyre::bail;
use futures_core::future::BoxFuture;

use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

pub use eyre::Result;
use libhttpclient::form_urlencoded;
use tracing::{debug, info, trace};

use config_types::RedditSecrets;
use libhttpclient::HttpClient;
use merde::CowStr;
use url::Url;

// cache the oauth token in a static variable
#[derive(Debug, Clone)]
struct AccessToken {
    token: String,
    expires_at: Instant,
}

struct ModImpl {
    client: Box<dyn HttpClient>,
    cached_token: Mutex<Option<AccessToken>>,
}

impl Default for ModImpl {
    fn default() -> Self {
        Self {
            client: libhttpclient::load().client(),
            cached_token: Mutex::new(None),
        }
    }
}

static MOD: LazyLock<ModImpl> = LazyLock::new(ModImpl::default);

pub fn load() -> &'static dyn Mod {
    &*MOD
}

#[autotrait]
impl Mod for ModImpl {
    fn get_submission<'fut>(
        &'fut self,
        secrets: &'fut RedditSecrets,
        url: &'fut str,
    ) -> BoxFuture<'fut, Result<Option<String>>> {
        Box::pin(async move {
            // get an application-only oauth token from reddit.
            let access_token: AccessToken;
            'get_access_token: {
                // note: this doesn't deduplicate in-flight requests but pfff

                // get it from the cache if it's not expired
                let cached_token = self.cached_token.lock().unwrap().clone();
                if let Some(token) = cached_token {
                    // add 1 minute for safety
                    if token.expires_at > (Instant::now() + std::time::Duration::from_secs(60)) {
                        access_token = token.clone();
                        info!("reusing reddit token");
                        break 'get_access_token;
                    } else {
                        info!("reddit token expired");
                    }
                } else {
                    info!("no reddit token in cache");
                }

                let api_uri = libhttpclient::Uri::builder()
                    .scheme("https")
                    .authority("www.reddit.com")
                    .path_and_query(
                        form_urlencoded::Serializer::new("/api/v1/access_token?".to_owned())
                            .append_pair("grant_type", "client_credentials")
                            .finish(),
                    )
                    .build()
                    .unwrap();

                let params = String::new();

                let res = self
                    .client
                    .post(api_uri.clone())
                    .form(params)
                    .polite_user_agent()
                    .basic_auth(&secrets.oauth_client_id, Some(&secrets.oauth_client_secret))
                    .send()
                    .await?;

                let status = res.status();
                if !status.is_success() {
                    let error = res
                        .text()
                        .await
                        .unwrap_or_else(|_| "Could not get error text".into());
                    bail!("got HTTP {status} for URL ({api_uri}), server said: {error}");
                }

                // example output:
                /*
                got reddit token token=Object {"access_token": String("(redacted)"), "expires_in": Number(86400), "scope": String("*"), "token_type": String("bearer")}
                */
                #[derive(Debug)]
                struct RedditAccessToken<'s> {
                    access_token: CowStr<'s>,
                    expires_in: u64,
                }
                merde::derive!(
                    impl (Deserialize) for struct RedditAccessToken<'s> { access_token, expires_in }
                );

                let token = res.json::<RedditAccessToken>().await?;
                trace!(?token, "got reddit token");

                let expires_at = Instant::now() + Duration::from_secs(token.expires_in);
                access_token = AccessToken {
                    token: token.access_token.into(),
                    expires_at,
                };
                // store a clone in cache, too
                *self.cached_token.lock().unwrap() = Some(access_token.clone());
            }

            let api_uri = libhttpclient::Uri::builder()
                .scheme("https")
                .authority("oauth.reddit.com")
                .path_and_query(
                    form_urlencoded::Serializer::new("/api/info/.json?".to_owned())
                        .append_pair("raw_json", "1")
                        .append_pair("url", url)
                        .finish(),
                )
                .build()
                .unwrap();

            let res = self
                .client
                .get(api_uri.clone())
                .polite_user_agent()
                .bearer_auth(&access_token.token)
                .send()
                .await?;

            let status = res.status();
            if !status.is_success() {
                let error = res
                    .text()
                    .await
                    .unwrap_or_else(|_| "Could not get error text".into());
                bail!("got HTTP {status} for URL ({api_uri}), server said: {error}");
            }

            #[derive(Debug)]
            struct Info<'s> {
                data: Listing<'s>,
            }

            merde::derive! {
                impl (Serialize, Deserialize) for struct Info<'s> { data }
            }

            #[derive(Debug)]
            struct Listing<'s> {
                children: Vec<Link<'s>>,
            }

            merde::derive! {
                impl (Serialize, Deserialize) for struct Listing<'s> { children }
            }

            #[derive(Debug)]
            struct Link<'s> {
                data: LinkData<'s>,
            }

            merde::derive! {
                impl (Serialize, Deserialize) for struct Link<'s> { data }
            }

            #[derive(Debug)]
            struct LinkData<'s> {
                subreddit: CowStr<'s>,
                permalink: CowStr<'s>,
            }

            merde::derive! {
                impl (Serialize, Deserialize) for struct LinkData<'s> { subreddit, permalink }
            }

            let info = res.json::<Info>().await?;

            debug!("info = {:#?}", info);

            let submission_url = info
                .data
                .children
                .iter()
                .find(|c| c.data.subreddit == "fasterthanlime")
                .map(|c| {
                    let mut u = Url::parse("https://www.reddit.com").unwrap();
                    u.set_path(&c.data.permalink);
                    u
                });

            match submission_url {
                Some(url) => {
                    // TODO: cache those in the database
                    info!(submission_url = %url, "found previous submission");
                    Ok(Some(url.to_string()))
                }
                None => {
                    info!("no previous submission found");
                    Ok(None)
                }
            }
        })
    }
}
