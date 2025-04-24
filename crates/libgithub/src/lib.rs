#![allow(non_snake_case)]

use autotrait::autotrait;
use futures_core::future::BoxFuture;
use libhttpclient::HttpClient;
use std::collections::HashSet;

use config_types::{RevisionConfig, TenantConfig, WebConfig};
use credentials::AuthBundle;
use eyre::Result;

mod impls;

#[derive(Default)]
struct ModImpl;

pub fn load() -> &'static dyn Mod {
    static MOD: ModImpl = ModImpl;
    &MOD
}

#[autotrait]
impl Mod for ModImpl {
    fn make_login_url(
        &self,
        tc: &TenantConfig,
        web: WebConfig,
        kind: GitHubLoginPurpose,
    ) -> eyre::Result<String> {
        use url::Url;
        let github_secrets = &tc.github_secrets()?;

        let mut u = Url::parse("https://github.com/login/oauth/authorize")?;
        {
            let mut q = u.query_pairs_mut();
            q.append_pair("response_type", "code");
            q.append_pair("client_id", &github_secrets.oauth_client_id);
            q.append_pair("redirect_uri", &impls::make_github_callback_url(tc, web));
            q.append_pair("scope", github_login_purpose_to_scopes(&kind));
        }
        Ok(u.to_string())
    }

    fn handle_oauth_callback<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        web: WebConfig,
        args: &'fut GitHubCallbackArgs,
    ) -> BoxFuture<'fut, Result<Option<GitHubCredentials>>> {
        Box::pin(async move { self.handle_oauth_callback_unboxed(tc, web, args).await })
    }

    fn to_auth_bundle<'fut>(
        &'fut self,
        rc: &'fut RevisionConfig,
        web: WebConfig,
        github_creds: GitHubCredentials,
    ) -> BoxFuture<'fut, Result<(GitHubCredentials, AuthBundle)>> {
        Box::pin(async move {
            self.to_site_credentials_unboxed(rc, web, &github_creds)
                .await
        })
    }

    fn list_sponsors<'fut>(
        &'fut self,
        tc: &'fut TenantConfig,
        client: &'fut dyn HttpClient,
        github_creds: &'fut GitHubCredentials,
    ) -> BoxFuture<'fut, Result<HashSet<String>>> {
        Box::pin(async move { self.list_sponsors_unboxed(tc, client, github_creds).await })
    }
}

#[derive(Debug, Clone)]
pub struct GitHubCallbackArgs {
    pub raw_query: String,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct GitHubCallbackArgs { raw_query }
}

#[derive(Debug, Clone)]
pub struct GitHubCallbackResponse {
    pub auth_bundle: AuthBundle,
    pub github_credentials: GitHubCredentials,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct GitHubCallbackResponse { auth_bundle, github_credentials }
}

#[derive(Debug, Clone)]
pub struct GitHubCredentials {
    /// example: "ajba90sd098w0e98f0w9e8g90a8ed098wgfae_w"
    pub access_token: String,
    /// example: "read:user"
    pub scope: String,
    /// example: "bearer"
    pub token_type: Option<String>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct GitHubCredentials { access_token, scope, token_type }
}
/// The purpose of the login (to determine the OAuth scopes needed for the login)
pub enum GitHubLoginPurpose {
    // admin login
    Admin,
    // normal user login
    Regular,
}

/// Returns GitHub OAuth scopes needed for the login
pub fn github_login_purpose_to_scopes(purpose: &GitHubLoginPurpose) -> &'static str {
    match purpose {
        GitHubLoginPurpose::Admin => "read:user,read:org",
        GitHubLoginPurpose::Regular => "read:user",
    }
}
