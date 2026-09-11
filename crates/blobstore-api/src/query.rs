/// Parses a raw query string (`a=b&c=d%20e`) into decoded `(name, value)` pairs.
/// Written by hand rather than pulling in a full URL crate, since we only need
/// this one operation and axum's `Query<T>` extractor doesn't fit the
/// "arbitrary `x-ms-*`/SAS query params, looked up by name" access pattern here.
pub fn parse_query(raw: &str) -> Vec<(String, String)> {
    if raw.is_empty() {
        return Vec::new();
    }
    raw.split('&')
        .filter(|p| !p.is_empty())
        .map(|pair| {
            let mut it = pair.splitn(2, '=');
            let k = it.next().unwrap_or("");
            let v = it.next().unwrap_or("");
            (
                urlencoding::decode(k)
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| k.to_string()),
                urlencoding::decode(v)
                    .map(|c| c.into_owned())
                    .unwrap_or_else(|_| v.to_string()),
            )
        })
        .collect()
}

pub fn get<'a>(params: &'a [(String, String)], name: &str) -> Option<&'a str> {
    params
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.as_str())
}
