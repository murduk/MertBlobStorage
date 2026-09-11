//! Shared domain types for the MertBlobStorage server: names, small value types,
//! and the canonical Azure error catalog. This crate has no dependency on HTTP,
//! storage, or auth so every other crate can depend on it without cycles.

pub mod error;
pub mod model;

pub use error::{AzureError, AzureErrorCode};
pub use model::{BlobType, PublicAccessLevel};

/// RFC 1123 date format Azure uses for `Date` / `Last-Modified` headers,
/// e.g. `Wed, 10 Sep 2026 12:34:56 GMT`.
pub fn format_http_date(t: time::OffsetDateTime) -> String {
    // time's well-known RFC 2822-ish formatting isn't quite RFC 1123, so we build it by hand
    // using fixed English names to avoid locale dependence.
    const WEEKDAYS: [&str; 7] = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
    const MONTHS: [&str; 12] = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ];
    let t = t.to_offset(time::UtcOffset::UTC);
    let weekday = WEEKDAYS[t.weekday().number_days_from_monday() as usize];
    let month = MONTHS[t.month() as u8 as usize - 1];
    format!(
        "{}, {:02} {} {:04} {:02}:{:02}:{:02} GMT",
        weekday,
        t.day(),
        month,
        t.year(),
        t.hour(),
        t.minute(),
        t.second()
    )
}

/// Generates an opaque, strictly-increasing-looking ETag value in the quoted
/// hex format Azure clients expect, e.g. `"0x8D1B4A2C3F4E5A0"`.
pub fn generate_etag() -> String {
    let now = time::OffsetDateTime::now_utc();
    let nanos = now.unix_timestamp_nanos();
    format!("\"0x{:X}\"", nanos as u128)
}
