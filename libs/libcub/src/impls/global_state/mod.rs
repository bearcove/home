use std::sync::OnceLock;

use crate::impls::types::CubGlobalState;

static GLOBAL_STATE: OnceLock<&'static CubGlobalState> = OnceLock::new();

/// Get a reference to the global state
pub fn global_state() -> &'static CubGlobalState {
    GLOBAL_STATE.get().expect("GLOBAL_STATE not initialized")
}

/// Set the global state (can only be done once)
pub fn set_global_state(gs: &'static CubGlobalState) -> Result<(), &'static CubGlobalState> {
    GLOBAL_STATE.set(gs)
}
