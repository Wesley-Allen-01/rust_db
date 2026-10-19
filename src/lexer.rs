//! Turns SQL text into tokens tagged with their line/column position.
//! Hand-written per ADR-0001; depends on nothing else in the crate.

use std::fmt;

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Keyword(Keyword),
    /// An unquoted identifier, folded to lowercase because names are
    /// case-insensitive.
    Ident(String),
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
pub struct Spanned {
    pub token: Token,
    pub pos: Position,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LexError {
    pub pos: Position,
}

impl fmt::Display for LexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}, col {}", self.pos.line, self.pos.col)
    }
}

impl std::error::Error for LexError {}

pub fn tokenize(sql: &str) -> Result<Vec<Spanned>, LexError> {
    Lexer::new(sql).run()
}

struct Lexer {
    chars: Vec<char>,
    i: usize,
    line: usize,
    col: usize,
}

impl Lexer {
    fn new(sql: &str) -> Lexer {
        Lexer {
            chars: sql.chars().collect(),
            i: 0,
            line: 1,
            col: 1,
        }
    }

    fn run(mut self) -> Result<Vec<Spanned>, LexError> {
        let mut out = Vec::new();
        while let Some(c) = self.peek() {
            if c.is_whitespace() {
                self.bump();
                continue;
            }
            let pos = self.pos();
            let token = self.word();
            out.push(Spanned { token, pos });
        }
        Ok(out)
    }

    fn word(&mut self) -> Token {
        let mut word = String::new();
        while let Some(c) = self.peek().filter(|c| c.is_alphanumeric() || *c == '_') {
            word.push(c);
            self.bump();
        }
        match Keyword::from_word(&word) {
            Some(keyword) => Token::Keyword(keyword),
            None => Token::Ident(word.to_lowercase()),
        }
    }

    fn peek(&self) -> Option<char> {
        self.chars.get(self.i).copied()
    }

    fn pos(&self) -> Position {
        Position {
            line: self.line,
            col: self.col,
        }
    }

    fn bump(&mut self) {
        if self.peek() == Some('\n') {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
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
    fn identifiers_fold_to_lowercase() {
        assert_eq!(
            tokens("Users my_col2 rowid"),
            vec![
                Token::Ident("users".into()),
                Token::Ident("my_col2".into()),
                Token::Ident("rowid".into()),
            ]
        );
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
