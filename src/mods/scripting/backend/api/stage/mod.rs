pub mod characters;
pub mod props;

use super::bindings::ScriptApiEntry;

pub(crate) fn api_entries() -> Vec<ScriptApiEntry> {
    let mut entries = characters::api_entries();
    entries.extend(props::api_entries());
    entries
}
