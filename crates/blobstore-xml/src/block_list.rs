use blobstore_core::{AzureError, AzureErrorCode};
use blobstore_storage::{BlockListResult, BlockRef, BlockRefKind};
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::xml_escape;

/// Parses a Put Block List request body:
/// ```xml
/// <?xml version="1.0" encoding="utf-8"?>
/// <BlockList>
///   <Committed>base64-block-id</Committed>
///   <Uncommitted>base64-block-id</Uncommitted>
///   <Latest>base64-block-id</Latest>
/// </BlockList>
/// ```
/// Uses `quick_xml`'s streaming reader (not serde) so the document order of
/// interleaved `<Committed>`/`<Uncommitted>`/`<Latest>` tags — which determines
/// the final blob's byte order — is preserved exactly.
pub fn parse_block_list_request(body: &[u8]) -> Result<Vec<BlockRef>, AzureError> {
    let mut reader = Reader::from_reader(body);
    reader.trim_text(true);

    let mut blocks = Vec::new();
    let mut current_kind: Option<BlockRefKind> = None;
    let mut buf = Vec::new();

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                current_kind = match e.local_name().as_ref() {
                    b"Committed" => Some(BlockRefKind::Committed),
                    b"Uncommitted" => Some(BlockRefKind::Uncommitted),
                    b"Latest" => Some(BlockRefKind::Latest),
                    _ => None,
                };
            }
            Ok(Event::Text(t)) => {
                if let Some(kind) = current_kind.take() {
                    let text = t
                        .unescape()
                        .map_err(|e| {
                            AzureError::with_message(
                                AzureErrorCode::InvalidXmlDocument,
                                e.to_string(),
                            )
                        })?
                        .into_owned();
                    blocks.push(BlockRef {
                        block_id: text,
                        kind,
                    });
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                return Err(AzureError::with_message(
                    AzureErrorCode::InvalidXmlDocument,
                    e.to_string(),
                ))
            }
            _ => {}
        }
        buf.clear();
    }

    if blocks.is_empty() {
        return Err(AzureErrorCode::InvalidBlockList.into());
    }

    Ok(blocks)
}

/// Renders the `BlockList` response for Get Block List.
pub fn render_block_list_response(result: &BlockListResult) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<BlockList>");
    out.push_str("<CommittedBlocks>");
    for b in &result.committed {
        out.push_str(&format!(
            "<Block><Name>{}</Name><Size>{}</Size></Block>",
            xml_escape(&b.block_id),
            b.size
        ));
    }
    out.push_str("</CommittedBlocks>");
    out.push_str("<UncommittedBlocks>");
    for b in &result.uncommitted {
        out.push_str(&format!(
            "<Block><Name>{}</Name><Size>{}</Size></Block>",
            xml_escape(&b.block_id),
            b.size
        ));
    }
    out.push_str("</UncommittedBlocks>");
    out.push_str("</BlockList>");
    out
}
