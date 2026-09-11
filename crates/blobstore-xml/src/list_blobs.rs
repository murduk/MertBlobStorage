use blobstore_storage::BlobListPage;

use crate::xml_escape;

/// Renders the `EnumerationResults` body for
/// `GET /{account}/{container}?restype=container&comp=list`.
#[allow(clippy::too_many_arguments)]
pub fn render_list_blobs(
    service_endpoint: &str,
    container: &str,
    prefix: Option<&str>,
    delimiter: Option<&str>,
    marker: Option<&str>,
    max_results: u32,
    page: &BlobListPage,
) -> String {
    let mut out = String::new();
    out.push_str("<?xml version=\"1.0\" encoding=\"utf-8\"?>\n");
    out.push_str(&format!(
        "<EnumerationResults ServiceEndpoint=\"{}\" ContainerName=\"{}\">",
        xml_escape(service_endpoint),
        xml_escape(container)
    ));
    out.push_str(&format!(
        "<Prefix>{}</Prefix>",
        xml_escape(prefix.unwrap_or(""))
    ));
    if let Some(d) = delimiter {
        out.push_str(&format!("<Delimiter>{}</Delimiter>", xml_escape(d)));
    }
    out.push_str(&format!(
        "<Marker>{}</Marker>",
        xml_escape(marker.unwrap_or(""))
    ));
    out.push_str(&format!("<MaxResults>{max_results}</MaxResults>"));
    out.push_str("<Blobs>");
    for entry in &page.blobs {
        let p = &entry.props;
        out.push_str("<Blob>");
        out.push_str(&format!("<Name>{}</Name>", xml_escape(&p.name)));
        out.push_str("<Properties>");
        out.push_str(&format!(
            "<Last-Modified>{}</Last-Modified>",
            p.last_modified
        ));
        out.push_str(&format!("<Etag>{}</Etag>", p.etag));
        out.push_str(&format!(
            "<Content-Length>{}</Content-Length>",
            p.content_length
        ));
        if let Some(ct) = &p.content_type {
            out.push_str(&format!("<Content-Type>{}</Content-Type>", xml_escape(ct)));
        }
        if let Some(md5) = &p.content_md5 {
            out.push_str(&format!("<Content-MD5>{}</Content-MD5>", xml_escape(md5)));
        }
        if let Some(v) = &p.content_encoding {
            out.push_str(&format!(
                "<Content-Encoding>{}</Content-Encoding>",
                xml_escape(v)
            ));
        }
        if let Some(v) = &p.content_language {
            out.push_str(&format!(
                "<Content-Language>{}</Content-Language>",
                xml_escape(v)
            ));
        }
        if let Some(v) = &p.cache_control {
            out.push_str(&format!("<Cache-Control>{}</Cache-Control>", xml_escape(v)));
        }
        if let Some(v) = &p.content_disposition {
            out.push_str(&format!(
                "<Content-Disposition>{}</Content-Disposition>",
                xml_escape(v)
            ));
        }
        out.push_str(&format!("<BlobType>{}</BlobType>", p.blob_type.as_str()));
        out.push_str("<LeaseStatus>unlocked</LeaseStatus>");
        out.push_str("<LeaseState>available</LeaseState>");
        out.push_str("</Properties>");
        out.push_str("<Metadata>");
        for (k, v) in &p.metadata {
            out.push_str(&format!(
                "<{}>{}</{}>",
                xml_escape(k),
                xml_escape(v),
                xml_escape(k)
            ));
        }
        out.push_str("</Metadata>");
        out.push_str("</Blob>");
    }
    for prefix_entry in &page.blob_prefixes {
        out.push_str(&format!(
            "<BlobPrefix><Name>{}</Name></BlobPrefix>",
            xml_escape(prefix_entry)
        ));
    }
    out.push_str("</Blobs>");
    out.push_str(&format!(
        "<NextMarker>{}</NextMarker>",
        xml_escape(page.next_marker.as_deref().unwrap_or(""))
    ));
    out.push_str("</EnumerationResults>");
    out
}
