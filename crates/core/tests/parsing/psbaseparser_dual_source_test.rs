use bolivar_core::psparser::{PSBaseParser, PSToken};
use std::sync::Arc;

fn collect_tokens(parser: &mut PSBaseParser) -> Vec<PSToken> {
    let mut out = Vec::new();
    while let Some(result) = parser.next_token() {
        let (_, token) = result.expect("tokenize");
        out.push(token);
    }
    out
}

#[test]
fn psbaseparser_supports_borrowed_and_shared_sources() {
    let data = b"1 2 3 (hi)";

    let mut borrowed = PSBaseParser::new(data);
    let shared_data = Arc::from(&data[..]);
    let mut shared = PSBaseParser::new_shared(shared_data);

    let borrowed_tokens = collect_tokens(&mut borrowed);
    let shared_tokens = collect_tokens(&mut shared);

    assert_eq!(borrowed_tokens, shared_tokens);
}
