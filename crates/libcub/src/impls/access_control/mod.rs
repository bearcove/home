use conflux::LoadedPage;
use cub_types::CubReq;

use super::cub_req::CubReqImpl;

#[derive(Debug, Clone, Copy)]
#[allow(dead_code)]
pub enum CanAccess {
    Yes(AccessGrantedReason),
    No(AccessDeniedReason),
}

#[derive(Debug, Clone, Copy)]
pub enum AccessGrantedReason {
    IsAdmin,
    NotDenied,
}

#[derive(Debug, Clone, Copy)]
#[allow(clippy::enum_variant_names)]
pub enum AccessDeniedReason {
    PageIsDraftAndDoesNotHaveDraftCode,
    PageIsDraftAndQueryDoesNotHaveDraftCode,
    PageIsDraftAndQueryDoesNotMatchDraftCode,
}

/// Determines if the current user can access a page based on its draft status,
/// draft code, and publication date.
pub(crate) fn can_access(rx: &CubReqImpl, page: &LoadedPage) -> eyre::Result<CanAccess> {
    if rx.viewer()?.is_admin {
        return Ok(CanAccess::Yes(AccessGrantedReason::IsAdmin));
    }

    if page.draft {
        let draft_code = match page.draft_code.as_deref() {
            Some(code) => code,
            None => {
                return Ok(CanAccess::No(
                    AccessDeniedReason::PageIsDraftAndDoesNotHaveDraftCode,
                ));
            }
        };

        let url_params = rx.url_params_map();
        let query_draft_code = match url_params.get("draft_code") {
            Some(code) => code,
            None => {
                return Ok(CanAccess::No(
                    AccessDeniedReason::PageIsDraftAndQueryDoesNotHaveDraftCode,
                ));
            }
        };

        if query_draft_code != draft_code {
            return Ok(CanAccess::No(
                AccessDeniedReason::PageIsDraftAndQueryDoesNotMatchDraftCode,
            ));
        }
    }

    Ok(CanAccess::Yes(AccessGrantedReason::NotDenied))
}
