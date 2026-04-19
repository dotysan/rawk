use crate::token::Token;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ParseErrorKind {
    ExpectedRule,
    ExpectedStatement,
    ExpectedIdentifier,
    UnsupportedStatement,
    ExpectedLeftParen,
    ExpectedLeftBrace,
    ExpectedRightSquareBracket,
    ExpectedComma,
    ExpectedColon,
    ExpectedSemicolon,
    ExpectedWhile,
    ExpectedRightBrace,
    ExpectedRightParen,
    MissingPrintfFormatString,
    InvalidNumericLiteral,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ParseError<'a> {
    pub kind: ParseErrorKind,
    pub token: Token<'a>,
}

impl std::fmt::Display for ParseError<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.kind {
            ParseErrorKind::ExpectedRule => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected rule",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedStatement => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected statement",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedIdentifier => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected identifier",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::UnsupportedStatement => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: unsupported statement syntax",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedLeftParen => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected left paren",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedLeftBrace => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected left brace",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedRightSquareBracket => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected right square bracket",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedComma => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected comma",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedColon => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected colon",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedSemicolon => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected semicolon",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedWhile => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected while",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedRightBrace => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected right brace",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::ExpectedRightParen => write!(
                f,
                "unexpected token {:?} ({:?}) at byte {}: expected right paren",
                self.token.kind, self.token.literal, self.token.span.start
            ),
            ParseErrorKind::MissingPrintfFormatString => write!(
                f,
                "printf requires a format string at byte {}",
                self.token.span.start
            ),
            ParseErrorKind::InvalidNumericLiteral => write!(
                f,
                "invalid numeric literal {:?} at byte {}",
                self.token.literal, self.token.span.start
            ),
        }
    }
}

impl std::error::Error for ParseError<'_> {}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::{Token, TokenKind};

    fn parse_error(kind: ParseErrorKind, token: Token<'static>) -> ParseError<'static> {
        ParseError { kind, token }
    }

    #[test]
    fn display_expected_rule_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedRule,
            Token::new(TokenKind::Else, "else", 12),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Else (\"else\") at byte 12: expected rule"
        );
    }

    #[test]
    fn display_expected_statement_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedStatement,
            Token::new(TokenKind::Else, "else", 4),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Else (\"else\") at byte 4: expected statement"
        );
    }

    #[test]
    fn display_expected_identifier_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedIdentifier,
            Token::new(TokenKind::Number, "1", 8),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Number (\"1\") at byte 8: expected identifier"
        );
    }

    #[test]
    fn display_unsupported_statement_error() {
        let err = parse_error(
            ParseErrorKind::UnsupportedStatement,
            Token::new(TokenKind::Plus, "+", 6),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Plus (\"+\") at byte 6: unsupported statement syntax"
        );
    }

    #[test]
    fn display_expected_left_paren_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedLeftParen,
            Token::new(TokenKind::Identifier, "x", 5),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Identifier (\"x\") at byte 5: expected left paren"
        );
    }

    #[test]
    fn display_expected_left_brace_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedLeftBrace,
            Token::new(TokenKind::Print, "print", 6),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Print (\"print\") at byte 6: expected left brace"
        );
    }

    #[test]
    fn display_expected_right_square_bracket_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedRightSquareBracket,
            Token::new(TokenKind::Assign, "=", 9),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Assign (\"=\") at byte 9: expected right square bracket"
        );
    }

    #[test]
    fn display_expected_comma_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedComma,
            Token::new(TokenKind::Identifier, "arr", 11),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Identifier (\"arr\") at byte 11: expected comma"
        );
    }

    #[test]
    fn display_expected_colon_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedColon,
            Token::new(TokenKind::RightCurlyBrace, "}", 13),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token RightCurlyBrace (\"}\") at byte 13: expected colon"
        );
    }

    #[test]
    fn display_expected_semicolon_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedSemicolon,
            Token::new(TokenKind::Identifier, "i", 15),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Identifier (\"i\") at byte 15: expected semicolon"
        );
    }

    #[test]
    fn display_expected_while_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedWhile,
            Token::new(TokenKind::RightCurlyBrace, "}", 7),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token RightCurlyBrace (\"}\") at byte 7: expected while"
        );
    }

    #[test]
    fn display_expected_right_paren_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedRightParen,
            Token::new(TokenKind::Print, "print", 10),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Print (\"print\") at byte 10: expected right paren"
        );
    }

    #[test]
    fn display_expected_right_brace_error() {
        let err = parse_error(
            ParseErrorKind::ExpectedRightBrace,
            Token::new(TokenKind::Eof, "", 16),
        );

        assert_eq!(
            format!("{err}"),
            "unexpected token Eof (\"\") at byte 16: expected right brace"
        );
    }

    #[test]
    fn display_missing_printf_format_string_error() {
        let err = parse_error(
            ParseErrorKind::MissingPrintfFormatString,
            Token::new(TokenKind::RightCurlyBrace, "}", 14),
        );

        assert_eq!(
            format!("{err}"),
            "printf requires a format string at byte 14"
        );
    }

    #[test]
    fn display_invalid_numeric_literal_error() {
        let err = parse_error(
            ParseErrorKind::InvalidNumericLiteral,
            Token::new(TokenKind::Number, "0xZZ", 3),
        );

        assert_eq!(
            format!("{err}"),
            "invalid numeric literal \"0xZZ\" at byte 3"
        );
    }
}
