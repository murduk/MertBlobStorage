use blobstore_storage::ContainerListPage;

use crate::xml_escape;

/// Renders the `EnumerationResults` body for `GET /{account}?comp=list`.
pub fn render_list_containers(
    service_endpoint: &str,
    prefix: Option<&str>,
    marker: Option<&str>,
    max_results: u32,
    page: &ContainerListPage,
) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str(&format!(
        "<EnumerationResults ServiceEndpoint=\"{}\">",
        xml_escape(service_endpoint)
    ));
    out.push_str(&format!(
        "<Prefix>{}</Prefix>",
        xml_escape(prefix.unwrap_or(""))
    ));
    out.push_str(&format!(
        "<Marker>{}</Marker>",
        xml_escape(marker.unwrap_or(""))
    ));
    out.push_str(&format!("<MaxResults>{max_results}</MaxResults>"));
    out.push_str("<Containers>");
    for c in &page.containers {
        out.push_str("<Container>");
        out.push_str(&format!("<Name>{}</Name>", xml_escape(&c.name)));
        out.push_str("<Properties>");
        out.push_str(&format!(
            "<Last-Modified>{}</Last-Modified>",
            c.last_modified
        ));
        out.push_str(&format!("<Etag>{}</Etag>", c.etag));
        out.push_str("<LeaseStatus>unlocked</LeaseStatus>");
        out.push_str("<LeaseState>available</LeaseState>");
        if let Some(pa) = c.public_access.as_header_value() {
            out.push_str(&format!("<PublicAccess>{pa}</PublicAccess>"));
        }
        out.push_str("</Properties>");
        out.push_str("<Metadata>");
        for (k, v) in &c.metadata {
            out.push_str(&format!(
                "<{}>{}</{}>",
                xml_escape(k),
                xml_escape(v),
                xml_escape(k)
            ));
        }
        out.push_str("</Metadata>");
        out.push_str("</Container>");
    }
    out.push_str("</Containers>");
    out.push_str(&format!(
        "<NextMarker>{}</NextMarker>",
        xml_escape(page.next_marker.as_deref().unwrap_or(""))
    ));
    out.push_str("</EnumerationResults>");
    out
}
