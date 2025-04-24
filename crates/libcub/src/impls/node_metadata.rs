use config_types::is_production;
use merde::IntoStatic as _;
use tracing::warn;

pub(crate) struct NodeMetadata {
    #[allow(dead_code)]
    pub(crate) node_type: String,
    pub(crate) region: String,
}

merde::derive!(
    impl (Serialize, Deserialize) for
    struct NodeMetadata { node_type, region }
);

pub(crate) async fn load_node_metadata() -> eyre::Result<NodeMetadata> {
    let node_metadata_path = "/metadata/node-metadata.json";
    let mut found_metadata = false;

    let metadata = if let Ok(metadata_content) = tokio::fs::read_to_string(node_metadata_path).await
    {
        found_metadata = true;
        merde::json::from_str_owned(&metadata_content).map_err(|e| e.into_static())?
    } else {
        NodeMetadata {
            node_type: "leader".into(),
            region: "unknown".into(),
        }
    };

    if is_production() && !found_metadata {
        warn!(
            "Expected metadata file to exist at {}, but it does not",
            node_metadata_path
        );
    }

    Ok(metadata)
}
