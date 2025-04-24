use merde::CowStr;
use serde::Serialize;
use std::sync::Arc;

use config_types::TenantInfo;

pub trait GlobalStateView: Send + Sync + 'static {
    fn gsv_sponsors(&self) -> Arc<Sponsors<'static>> {
        unimplemented!()
    }

    fn gsv_ti(&self) -> Arc<TenantInfo> {
        unimplemented!()
    }
}

#[derive(Clone, Serialize)]
pub struct Sponsors<'s> {
    pub sponsors: Vec<CowStr<'s>>,
}

merde::derive! {
    impl (Serialize, Deserialize) for struct Sponsors<'s> { sponsors }
}
