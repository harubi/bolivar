use bolivar_core::psparser::{ContentLexer, Keyword, PSToken};

fn collect_tokens(data: &[u8]) -> Vec<PSToken> {
    let mut lexer = ContentLexer::new(data);
    let mut tokens = Vec::new();
    while let Some(result) = lexer.next_token() {
        let (_, token) = result.expect("tokenize");
        tokens.push(token);
    }
    tokens
}

#[test]
fn test_content_lexer_basic_tokens() {
    let data = b"BT /F1 12 Tf (Hello) Tj ET";
    let tokens = collect_tokens(data);

    assert_eq!(
        tokens,
        vec![
            PSToken::Keyword(Keyword::BT),
            PSToken::Literal("F1".to_string()),
            PSToken::Int(12),
            PSToken::Keyword(Keyword::Tf),
            PSToken::String(b"Hello".to_vec()),
            PSToken::Keyword(Keyword::Tj),
            PSToken::Keyword(Keyword::ET),
        ]
    );
}

#[test]
fn test_content_lexer_hex_string_whitespace() {
    let data = b"<48 65 6C 6C 6F> Tj";
    let tokens = collect_tokens(data);

    assert_eq!(
        tokens,
        vec![
            PSToken::String(b"Hello".to_vec()),
            PSToken::Keyword(Keyword::Tj)
        ]
    );
}

#[test]
fn test_content_lexer_hex_string_odd_digits() {
    let data = b"<4F3> Tj";
    let tokens = collect_tokens(data);

    assert_eq!(
        tokens,
        vec![
            PSToken::String(vec![0x4f, 0x03]),
            PSToken::Keyword(Keyword::Tj)
        ]
    );
}

#[test]
fn test_content_lexer_literal_hex_escape() {
    let data = b"/foo#5fbar";
    let tokens = collect_tokens(data);
    assert_eq!(tokens, vec![PSToken::Literal("foo_bar".to_string())]);
}

#[test]
fn test_content_lexer_skips_comments() {
    let data = b"% comment\nBT";
    let tokens = collect_tokens(data);
    assert_eq!(tokens, vec![PSToken::Keyword(Keyword::BT)]);
}
