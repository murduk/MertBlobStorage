#[cfg(test)]
mod tests {
    use crate::shared_key::{string_to_sign, verify, SharedKeyRequest};

    fn base_request() -> SharedKeyRequest {
        SharedKeyRequest {
            verb: "GET".to_string(),
            content_length: "0".to_string(),
            date: String::new(),
            x_ms_headers: vec![
                (
                    "x-ms-date".to_string(),
                    "Wed, 10 Sep 2026 12:00:00 GMT".to_string(),
                ),
                ("x-ms-version".to_string(), "2021-08-06".to_string()),
            ],
            canonical_path: "/mycontainer".to_string(),
            query_params: vec![
                ("restype".to_string(), "container".to_string()),
                ("comp".to_string(), "list".to_string()),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn content_length_zero_becomes_empty_string() {
        let req = base_request();
        let sts = string_to_sign("myaccount", &req);
        // Content-Length is the 4th line (index 3) and must be empty when 0.
        let lines: Vec<&str> = sts.split('\n').collect();
        assert_eq!(lines[3], "");
    }

    #[test]
    fn canonicalized_headers_are_sorted_and_lowercased() {
        let req = base_request();
        let sts = string_to_sign("myaccount", &req);
        assert!(sts.contains("x-ms-date:Wed, 10 Sep 2026 12:00:00 GMT\nx-ms-version:2021-08-06\n"));
    }

    #[test]
    fn canonicalized_resource_includes_sorted_query_params() {
        let req = base_request();
        let sts = string_to_sign("myaccount", &req);
        assert!(sts.ends_with("/myaccount/mycontainer\ncomp:list\nrestype:container"));
    }

    #[test]
    fn round_trip_sign_and_verify_succeeds_with_matching_key() {
        let req = base_request();
        let key = "MTIzNDU2Nzg5MDEyMzQ1Njc4OTAxMjM0NTY3ODkwMTI="; // arbitrary valid base64
        let sts = string_to_sign("myaccount", &req);
        // Sign it ourselves the same way `verify` would, then check verify accepts it.
        use base64::engine::general_purpose::STANDARD as B64;
        use base64::Engine;
        use hmac::{Hmac, Mac};
        use sha2::Sha256;
        let key_bytes = B64.decode(key).unwrap();
        let mut mac = Hmac::<Sha256>::new_from_slice(&key_bytes).unwrap();
        mac.update(sts.as_bytes());
        let sig = B64.encode(mac.finalize().into_bytes());

        assert!(verify("myaccount", &req, &sig, key, None).is_ok());
    }

    #[test]
    fn verify_rejects_wrong_signature() {
        let req = base_request();
        let key = "MTIzNDU2Nzg5MDEyMzQ1Njc4OTAxMjM0NTY3ODkwMTI=";
        assert!(verify("myaccount", &req, "bm90LWEtcmVhbC1zaWc=", key, None).is_err());
    }
}
