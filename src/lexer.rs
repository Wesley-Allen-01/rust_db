//! Turns SQL text into tokens tagged with their line/column position.
//! Hand-written per ADR-0001; depends on nothing else in the crate.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Keyword(Keyword),
    /// An unquoted identifier, folded to lowercase because names are
    /// case-insensitive.
    Ident(String),
    Integer(i64),
    Float(f64),
    /// A single-quoted text literal, with `''` escapes already resolved.
    Text(String),
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    /// `||`, text concatenation.
    Concat,
    LParen,
    RParen,
    Comma,
    Semicolon,
    Dot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Keyword {
    Select,
    From,
    Where,
    Insert,
    Into,
    Values,
    Create,
    Table,
    Drop,
    Update,
    Set,
    Delete,
    Order,
    By,
    Limit,
    Offset,
    Asc,
    Desc,
    True,
    False,
    Null,
    Not,
    And,
    Or,
    Is,
    If,
    Exists,
    As,
    Int,
    Float,
    Text,
    Bool,
}

impl Keyword {
    fn from_word(word: &str) -> Option<Keyword> {
        let keyword = match word.to_ascii_uppercase().as_str() {
            "SELECT" => Keyword::Select,
            "FROM" => Keyword::From,
            "WHERE" => Keyword::Where,
            "INSERT" => Keyword::Insert,
            "INTO" => Keyword::Into,
            "VALUES" => Keyword::Values,
            "CREATE" => Keyword::Create,
            "TABLE" => Keyword::Table,
            "DROP" => Keyword::Drop,
            "UPDATE" => Keyword::Update,
            "SET" => Keyword::Set,
            "DELETE" => Keyword::Delete,
            "ORDER" => Keyword::Order,
            "BY" => Keyword::By,
            "LIMIT" => Keyword::Limit,
            "OFFSET" => Keyword::Offset,
            "ASC" => Keyword::Asc,
            "DESC" => Keyword::Desc,
            "TRUE" => Keyword::True,
            "FALSE" => Keyword::False,
            "NULL" => Keyword::Null,
            "NOT" => Keyword::Not,
            "AND" => Keyword::And,
            "OR" => Keyword::Or,
            "IS" => Keyword::Is,
            "IF" => Keyword::If,
            "EXISTS" => Keyword::Exists,
            "AS" => Keyword::As,
            "INT" => Keyword::Int,
            "FLOAT" => Keyword::Float,
            "TEXT" => Keyword::Text,
            "BOOL" => Keyword::Bool,
            _ => return None,
        };
        Some(keyword)
    }
}

/// 1-based line and column of a token's first character.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Position {
    pub line: usize,
    pub col: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PositionedToken {
    pub token: Token,
    pub pos: Position,
    /// Byte offset of the token's first character, for slicing the source.
    pub offset: usize,
}

/// Kept separate from `DbError` so the lexer stays free of crate
/// dependencies; the parser decides how to surface it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub pos: Position,
    pub kind: LexErrorKind,
}

impl LexError {
    fn new(pos: Position, kind: LexErrorKind) -> LexError {
        LexError { pos, kind }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LexErrorKind {
    UnexpectedChar(char),
    NumberOutOfRange,
    UnterminatedString,
    QuotedIdentifier,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, col {}: {}",
            self.pos.line, self.pos.col, self.kind
        )
    }
}

impl fmt::Display for LexErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LexErrorKind::UnexpectedChar(c) => write!(f, "unexpected character '{c}'"),
            LexErrorKind::NumberOutOfRange => write!(f, "numeric literal out of range"),
            LexErrorKind::UnterminatedString => write!(f, "unterminated string literal"),
            LexErrorKind::QuotedIdentifier => {
                write!(f, "double-quoted identifiers are not supported")
            }
        }
    }
}

impl std::error::Error for LexError {}

pub fn tokenize(sql: &str) -> Result<Vec<PositionedToken>, LexError> {
    Lexer::new(sql).run()
}

fn is_word_char(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    /// Where `chars[i]` sits in the source.
    pos: Position,
    /// The byte offset of `chars[i]`.
    offset: usize,
}

impl Lexer {
    fn new(sql: &str) -> Lexer {
        Lexer {
            chars: sql.chars().collect(),
            i: 0,
            pos: Position { line: 1, col: 1 },
            offset: 0,
        }
    }

    fn run(mut self) -> Result<Vec<PositionedToken>, LexError> {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
                continue;
            }
            if c == '-' && self.peek_next() == Some('-') {
                self.skip_comment();
                continue;
            }
            let (pos, offset) = (self.pos, self.offset);
            let token = if c.is_alphabetic() || c == '_' {
                self.word()
            } else if c == '\'' {
                self.text(pos)?
            } else if c.is_ascii_digit() {
                self.number(pos)?
            } else {
                self.symbol(c, pos)?
            };
            out.push(PositionedToken { token, pos, offset });
        }
        Ok(out)
    }

    fn word(&mut self) -> Token {
        let mut word = String::new();
        while let Some(c) = self.peek().filter(|c| is_word_char(*c)) {
            word.push(c);
            self.bump();
        }
        match Keyword::from_word(&word) {
            Some(keyword) => Token::Keyword(keyword),
            None => Token::Ident(word.to_lowercase()),
        }
    }

    fn number(&mut self, pos: Position) -> Result<Token, LexError> {
        let mut text = self.digits();
        // `1.x` is `1` then `.`; only a digit after the dot makes a float.
        let digit_after_dot = self.peek_next().is_some_and(|c| c.is_ascii_digit());
        let is_float = self.peek() == Some('.') && digit_after_dot;
        if is_float {
            self.bump();
            text.push('.');
            text.push_str(&self.digits());
        }
        // Without this, `123abc`, `1e5` or `1.5.3` would quietly split into
        // several tokens.
        let glued = |c: char| is_word_char(c) || (is_float && c == '.');
        if let Some(c) = self.peek().filter(|c| glued(*c)) {
            return Err(LexError::new(self.pos, LexErrorKind::UnexpectedChar(c)));
        }
        let out_of_range = || LexError::new(pos, LexErrorKind::NumberOutOfRange);
        if is_float {
            // Digits and one dot always parse, but a huge value parses as
            // infinity, which is not a number the user wrote.
            let value: f64 = text.parse().unwrap();
            if !value.is_finite() {
                return Err(out_of_range());
            }
            return Ok(Token::Float(value));
        }
        // Only digits remain, so the parse can fail only by overflowing.
        text.parse().map(Token::Integer).map_err(|_| out_of_range())
    }

    fn text(&mut self, pos: Position) -> Result<Token, LexError> {
        self.bump(); // past the opening quote; `pos` still points at it for errors
        let mut text = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(LexError::new(pos, LexErrorKind::UnterminatedString));
                }
                Some('\'') => {
                    self.bump();
                    if !self.eat('\'') {
                        return Ok(Token::Text(text));
                    }
                    text.push('\'');
                }
                Some(c) => {
                    text.push(c);
                    self.bump();
                }
            }
        }
    }

    fn skip_comment(&mut self) {
        while self.peek().is_some_and(|c| c != '\n') {
            self.bump();
        }
    }

    fn digits(&mut self) -> String {
        let mut digits = String::new();
        while let Some(c) = self.peek().filter(char::is_ascii_digit) {
            digits.push(c);
            self.bump();
        }
        digits
    }

    fn symbol(&mut self, c: char, pos: Position) -> Result<Token, LexError> {
        self.bump();
        let token = match c {
            '+' => Token::Plus,
            '-' => Token::Minus,
            '*' => Token::Star,
            '/' => Token::Slash,
            '%' => Token::Percent,
            '=' => Token::Eq,
            '(' => Token::LParen,
            ')' => Token::RParen,
            ',' => Token::Comma,
            ';' => Token::Semicolon,
            '.' => Token::Dot,
            // Rejected outright so names stay case-insensitive everywhere.
            '"' => {
                return Err(LexError::new(pos, LexErrorKind::QuotedIdentifier));
            }
            '<' if self.eat('=') => Token::LtEq,
            '<' if self.eat('>') => Token::NotEq,
            '<' => Token::Lt,
            '>' if self.eat('=') => Token::GtEq,
            '>' => Token::Gt,
            '|' if self.eat('|') => Token::Concat,
            _ => {
                return Err(LexError::new(pos, LexErrorKind::UnexpectedChar(c)));
            }
        };
        Ok(token)
    }

    /// Lets two-character operators like `<=` be matched in a single arm.
    fn eat(&mut self, expected: char) -> bool {
        if self.peek() == Some(expected) {
            self.bump();
            true
        } else {
            false
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn peek_next(&self) -> Option<char> {
        self.chars.get(self.i + 1).copied()
    }

    fn bump(&mut self) {
        self.offset += self.peek().map_or(0, char::len_utf8);
        if self.peek() == Some('\n') {
            self.pos.line += 1;
            self.pos.col = 1;
        } else {
            self.pos.col += 1;
        }
        self.i += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(sql: &str) -> Vec<Token> {
        tokenize(sql)
            .expect("should lex")
            .into_iter()
            .map(|t| t.token)
            .collect()
    }

    fn error(sql: &str) -> LexError {
        tokenize(sql).expect_err("should fail to lex")
    }

    fn at(line: usize, col: usize) -> Position {
        Position { line, col }
    }

    #[test]
    fn keywords_are_case_insensitive() {
        assert_eq!(
            tokens("SELECT select SeLeCt"),
            vec![Token::Keyword(Keyword::Select); 3]
        );
    }

    #[test]
    fn recognises_every_keyword() {
        use Keyword::*;
        let sql = "select from where insert into values create table drop \
                   update set delete order by limit offset asc desc \
                   true false null not and or is if exists as \
                   int float text bool";
        let expected = [
            Select, From, Where, Insert, Into, Values, Create, Table, Drop, Update, Set, Delete,
            Order, By, Limit, Offset, Asc, Desc, True, False, Null, Not, And, Or, Is, If, Exists,
            As, Int, Float, Text, Bool,
        ];
        assert_eq!(tokens(sql), expected.map(Token::Keyword).to_vec());
    }

    #[test]
    fn recognises_every_operator_and_punctuation() {
        use Token::*;
        assert_eq!(
            tokens("+ - * / % = <> < <= > >= || ( ) , ; ."),
            vec![
                Plus, Minus, Star, Slash, Percent, Eq, NotEq, Lt, LtEq, Gt, GtEq, Concat, LParen,
                RParen, Comma, Semicolon, Dot,
            ]
        );
    }

    #[test]
    fn operators_need_no_surrounding_space() {
        use Token::*;
        assert_eq!(
            tokens("a<=b<>c||d>e"),
            vec![
                Ident("a".into()),
                LtEq,
                Ident("b".into()),
                NotEq,
                Ident("c".into()),
                Concat,
                Ident("d".into()),
                Gt,
                Ident("e".into()),
            ]
        );
    }

    #[test]
    fn lexes_integer_and_float_literals() {
        assert_eq!(
            tokens("42 2.5 0 007"),
            vec![
                Token::Integer(42),
                Token::Float(2.5),
                Token::Integer(0),
                Token::Integer(7),
            ]
        );
    }

    #[test]
    fn a_dot_not_followed_by_a_digit_ends_the_number() {
        assert_eq!(
            tokens("1.x"),
            vec![Token::Integer(1), Token::Dot, Token::Ident("x".into())]
        );
    }

    #[test]
    fn rejects_a_letter_straight_after_a_number() {
        let err = error("select 123abc");
        assert_eq!(err.kind, LexErrorKind::UnexpectedChar('a'));
        assert_eq!(err.pos, at(1, 11));

        // Exponents aren't supported, so they must fail rather than lex as `1`, `e5`.
        let err = error("1e5");
        assert_eq!(err.kind, LexErrorKind::UnexpectedChar('e'));
        assert_eq!(err.pos, at(1, 2));
    }

    #[test]
    fn rejects_a_float_too_large_for_64_bits() {
        let sql = format!("select {}.0", "9".repeat(400));
        let err = error(&sql);
        assert_eq!(err.kind, LexErrorKind::NumberOutOfRange);
        assert_eq!(err.pos, at(1, 8));
    }

    #[test]
    fn rejects_a_second_dot_in_a_number() {
        let err = error("select 1.5.3");
        assert_eq!(err.kind, LexErrorKind::UnexpectedChar('.'));
        assert_eq!(err.pos, at(1, 11));
    }

    #[test]
    fn rejects_an_integer_too_large_for_64_bits() {
        let err = error("select 9223372036854775808");
        assert_eq!(err.kind, LexErrorKind::NumberOutOfRange);
        assert_eq!(err.pos, at(1, 8));
        assert_eq!(
            err.to_string(),
            "line 1, col 8: numeric literal out of range"
        );
    }

    #[test]
    fn lexes_text_literals_keeping_case_and_spaces() {
        assert_eq!(
            tokens("'Hello, World' ''"),
            vec![Token::Text("Hello, World".into()), Token::Text("".into())]
        );
    }

    #[test]
    fn doubled_single_quote_is_an_escaped_quote() {
        assert_eq!(tokens("'it''s'"), vec![Token::Text("it's".into())]);
    }

    #[test]
    fn text_literals_can_span_lines() {
        let lexed = tokenize("'a\nb' x").unwrap();
        assert_eq!(lexed[0].token, Token::Text("a\nb".into()));
        assert_eq!(lexed[1].pos, at(2, 4));
    }

    #[test]
    fn rejects_an_unterminated_string_at_its_opening_quote() {
        let err = error("select 'abc''");
        assert_eq!(err.kind, LexErrorKind::UnterminatedString);
        assert_eq!(err.pos, at(1, 8));
        assert_eq!(
            err.to_string(),
            "line 1, col 8: unterminated string literal"
        );
    }

    #[test]
    fn rejects_a_double_quoted_identifier() {
        let err = error("select \"Name\" from t");
        assert_eq!(err.kind, LexErrorKind::QuotedIdentifier);
        assert_eq!(err.pos, at(1, 8));
        assert_eq!(
            err.to_string(),
            "line 1, col 8: double-quoted identifiers are not supported"
        );
    }

    #[test]
    fn comments_run_to_the_end_of_the_line() {
        let sql = "select -- the rest is ignored: @ 'x\n  a -- trailing";
        assert_eq!(
            tokens(sql),
            vec![Token::Keyword(Keyword::Select), Token::Ident("a".into())]
        );
        assert_eq!(tokenize(sql).unwrap()[1].pos, at(2, 3));
    }

    #[test]
    fn a_single_dash_is_still_minus() {
        assert_eq!(
            tokens("a - -b"),
            vec![
                Token::Ident("a".into()),
                Token::Minus,
                Token::Minus,
                Token::Ident("b".into()),
            ]
        );
    }

    #[test]
    fn lexes_a_whole_statement_with_positions() {
        use Token::{Comma, Concat, Float, Ident, Integer, Plus, Semicolon, Text};
        let lexed = tokenize("SELECT 1 + 2.5 AS x, 'a' || name;").unwrap();
        let got: Vec<(Token, usize)> = lexed.into_iter().map(|t| (t.token, t.pos.col)).collect();
        assert_eq!(
            got,
            vec![
                (Token::Keyword(Keyword::Select), 1),
                (Integer(1), 8),
                (Plus, 10),
                (Float(2.5), 12),
                (Token::Keyword(Keyword::As), 16),
                (Ident("x".into()), 19),
                (Comma, 20),
                (Text("a".into()), 22),
                (Concat, 26),
                (Ident("name".into()), 29),
                (Semicolon, 33),
            ]
        );
    }

    #[test]
    fn empty_and_blank_input_give_no_tokens() {
        assert_eq!(tokens(""), vec![]);
        assert_eq!(tokens("  \n -- only a comment"), vec![]);
    }

    #[test]
    fn rejects_an_unexpected_character() {
        let err = error("select a,\n  @x");
        assert_eq!(err.kind, LexErrorKind::UnexpectedChar('@'));
        assert_eq!(err.pos, at(2, 3));
        assert_eq!(err.to_string(), "line 2, col 3: unexpected character '@'");
    }

    #[test]
    fn rejects_a_lone_pipe() {
        let err = error("a | b");
        assert_eq!(err.kind, LexErrorKind::UnexpectedChar('|'));
        assert_eq!(err.pos, at(1, 3));
    }

    #[test]
    fn identifiers_fold_to_lowercase() {
        assert_eq!(
            // `rowid` must stay a plain name so the binder can reject it as
            // an unknown column rather than the parser choking on a keyword.
            tokens("Users my_col2 rowid"),
            vec![
                Token::Ident("users".into()),
                Token::Ident("my_col2".into()),
                Token::Ident("rowid".into()),
            ]
        );
    }

    #[test]
    fn tokens_carry_their_byte_offset() {
        // `é` is two bytes, so offsets and columns part ways after it.
        let offsets: Vec<usize> = tokenize("é;\n 'ü' x")
            .unwrap()
            .into_iter()
            .map(|t| t.offset)
            .collect();
        assert_eq!(offsets, vec![0, 2, 5, 10]);
    }

    #[test]
    fn tokens_carry_line_and_column() {
        let positions: Vec<(usize, usize)> = tokenize("select\n  name\n\tx")
            .unwrap()
            .into_iter()
            .map(|t| (t.pos.line, t.pos.col))
            .collect();
        assert_eq!(positions, vec![(1, 1), (2, 3), (3, 2)]);
    }
}
