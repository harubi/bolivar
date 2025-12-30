use bolivar::pdfinterp::PDFContentParser;

#[test]
fn test_pending_queue_order() {
    // Arrange for pending by leaving trailing operands (no keyword)
    let mut parser = PDFContentParser::new(vec![b"12 34".to_vec()]);

    let first = parser.next_with_pos();
    let second = parser.next_with_pos();
    let third = parser.next_with_pos();

    assert!(first.is_some());
    assert!(second.is_some());
    assert!(third.is_none());
}
