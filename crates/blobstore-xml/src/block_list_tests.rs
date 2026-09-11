#[cfg(test)]
mod tests {
    use crate::block_list::parse_block_list_request;
    use blobstore_storage::BlockRefKind;

    #[test]
    fn preserves_interleaved_tag_order() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?>
<BlockList>
  <Latest>AAAA</Latest>
  <Committed>BBBB</Committed>
  <Latest>CCCC</Latest>
</BlockList>"#;
        let refs = parse_block_list_request(xml).unwrap();
        let ids: Vec<&str> = refs.iter().map(|r| r.block_id.as_str()).collect();
        assert_eq!(ids, vec!["AAAA", "BBBB", "CCCC"]);
        assert!(matches!(refs[0].kind, BlockRefKind::Latest));
        assert!(matches!(refs[1].kind, BlockRefKind::Committed));
        assert!(matches!(refs[2].kind, BlockRefKind::Latest));
    }

    #[test]
    fn empty_block_list_is_rejected() {
        let xml = br#"<?xml version="1.0" encoding="utf-8"?><BlockList></BlockList>"#;
        assert!(parse_block_list_request(xml).is_err());
    }
}
