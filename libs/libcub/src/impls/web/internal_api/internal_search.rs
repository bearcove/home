use std::collections::HashMap;

use crate::impls::{
    cub_req::CubReqImpl,
    reply::{IntoLegacyReply, LegacyReply, MerdeJson},
};
use conflux::Input;
use cub_types::CubTenant;

pub(crate) async fn search_assets(
    query: axum::extract::Query<HashMap<String, String>>,
    tr: CubReqImpl,
) -> LegacyReply {
    let q = match query.get("q") {
        Some(value) => value,
        None => return http::StatusCode::BAD_REQUEST.into_legacy_reply(),
    };

    let irev = tr.tenant.rev()?;

    let routes: Vec<(String, String)> = irev
        .rev
        .assets
        .iter()
        .filter(|(route, _ar)| route.as_str().contains(q))
        .map(|(route, _ar)| {
            (
                route.as_str().to_string(),
                match _ar {
                    conflux::Asset::Inline {
                        content,
                        content_type,
                    } => format!("{content_type} ({} bytes)", content.len()),
                    conflux::Asset::Derivation(derivation) => {
                        format!("{derivation:?}")
                    }
                    conflux::Asset::AcceptBasedRedirect { options } => {
                        format!("AcceptBasedRedirect ({} options)", options.len())
                    }
                },
            )
        })
        .collect();

    MerdeJson(routes).into_legacy_reply()
}

pub(crate) async fn search_inputs(
    query: axum::extract::Query<HashMap<String, String>>,
    tr: CubReqImpl,
) -> LegacyReply {
    let q = match query.get("q") {
        Some(value) => value,
        None => return http::StatusCode::BAD_REQUEST.into_legacy_reply(),
    };

    let irev = tr.tenant.rev()?;
    let inputs: Vec<(String, Input)> = irev
        .rev
        .pak
        .inputs
        .iter()
        .filter(|(input, _value)| input.as_str().contains(q))
        .map(|(path, input)| (path.to_string(), input.clone()))
        .collect();

    MerdeJson(inputs).into_legacy_reply()
}
