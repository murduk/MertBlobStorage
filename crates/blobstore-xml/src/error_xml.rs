use blobstore_core::AzureError;

use crate::xml_escape;

/// Renders the standard Azure `<Error>` response body:
/// ```xml
/// <?xml version="1.0" encoding="utf-8"?>
/// <Error>
///   <Code>ContainerNotFound</Code>
///   <Message>The specified container does not exist.
/// RequestId:{uuid}
/// Time:{iso8601}</Message>
/// </Error>
/// ```
pub fn render_error(err: &AzureError, request_id: &str) -> String {
    let time = time::OffsetDateTime::now_utc()
        .format(&time::format_description::well_known::Rfc3339)
        .unwrap_or_default();
    format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n<Error><Code>{}</Code><Message>{}\nRequestId:{}\nTime:{}</Message></Error>",
        err.code.code_str(),
        xml_escape(&err.message),
        request_id,
        time,
    )
}
