use bolivar_core::pdfparser::PDFParser;
use bolivar_core::psparser::{PSBaseParser, PSStackParser};

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn parser_types_are_send_and_sync_when_backed_by_shared_bytes() {
    assert_send_sync::<PSBaseParser<'static>>();
    assert_send_sync::<PSStackParser<'static>>();
    assert_send_sync::<PDFParser<'static>>();
}
