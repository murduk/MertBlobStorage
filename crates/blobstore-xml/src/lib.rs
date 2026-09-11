//! Azure Blob Storage XML wire format: response bodies are hand-built (rather
//! than derived via serde) so the exact element order/casing Azure's real
//! service produces — which some clients parse positionally — is fully under
//! our control. Request bodies with order-sensitive semantics (Put Block List)
//! are parsed with `quick_xml`'s streaming reader directly, since a
//! struct-per-tag serde model would silently lose the interleaving of
//! `<Latest>`/`<Committed>`/`<Uncommitted>` entries that determines the final
//! blob's byte order.

mod block_list;
#[cfg(test)]
mod block_list_tests;
mod error_xml;
mod list_blobs;
mod list_containers;

pub use block_list::{parse_block_list_request, render_block_list_response};
pub use error_xml::render_error;
pub use list_blobs::render_list_blobs;
pub use list_containers::render_list_containers;

/// Escapes `&`, `<`, `>`, `"` for safe inclusion in XML text/attribute content.
pub fn xml_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(c),
        }
    }
    out
}
