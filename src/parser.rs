//! Turns SQL text into one Statement.
//!
//! Statements are parsed by recursive descent, expressions by precedence
//! climbing (Pratt parsing). Hand-written per ADR-0001.

use crate::ast::{BinaryOp, Expr, Select, SelectItem, Statement, UnaryOp};
use crate::error::{DbError, Result};
use crate::lexer::{Keyword, Position, PositionedToken, Token, tokenize};
use crate::value::Value;

/// Parses exactly one Statement, optionally ending in `;`.
///
/// Anything after the Statement, such as a second Statement, is an error.
pub fn parse(sql: &str) -> Result<Statement> {
    let tokens = tokenize(sql).map_err(|err| DbError::Syntax {
        pos: err.pos,
        message: err.kind.to_string(),
    })?;
    let mut parser = Parser {
        tokens,
        i: 0,
        end: end_of(sql),
    };
    let statement = parser.statement()?;
    parser.eat(&Token::Semicolon);
    if parser.peek().is_some() {
        return Err(parser.error("expected end of statement"));
    }
    Ok(statement)
}

/// The position just past the last character, where "end of input" errors point.
fn end_of(sql: &str) -> Position {
    let mut pos = Position { line: 1, col: 1 };
    for c in sql.chars() {
        if c == '\n' {
            pos = Position {
                line: pos.line + 1,
                col: 1,
            };
        } else {
            pos.col += 1;
        }
    }
    pos
}

/// How tightly each operator binds, loosest first. Follows Postgres's precedence.
mod power {
    use crate::ast::BinaryOp;

    pub const OR: u8 = 1;
    pub const AND: u8 = 2;
    pub const NOT: u8 = 3;
    pub const IS: u8 = 4;
    pub const COMPARE: u8 = 5;
    pub const CONCAT: u8 = 6;
    pub const ADD: u8 = 7;
    pub const MUL: u8 = 8;
    pub const NEG: u8 = 9;

    pub fn of(op: BinaryOp) -> u8 {
        match op {
            BinaryOp::Or => OR,
            BinaryOp::And => AND,
            BinaryOp::Eq
            | BinaryOp::NotEq
            | BinaryOp::Lt
            | BinaryOp::LtEq
            | BinaryOp::Gt
            | BinaryOp::GtEq => COMPARE,
            BinaryOp::Concat => CONCAT,
            BinaryOp::Add | BinaryOp::Sub => ADD,
            BinaryOp::Mul | BinaryOp::Div | BinaryOp::Mod => MUL,
        }
    }
}

fn binary_op(token: &Token) -> Option<BinaryOp> {
    let op = match token {
        Token::Plus => BinaryOp::Add,
        Token::Minus => BinaryOp::Sub,
        Token::Star => BinaryOp::Mul,
        Token::Slash => BinaryOp::Div,
        Token::Percent => BinaryOp::Mod,
        Token::Eq => BinaryOp::Eq,
        Token::NotEq => BinaryOp::NotEq,
        Token::Lt => BinaryOp::Lt,
        Token::LtEq => BinaryOp::LtEq,
        Token::Gt => BinaryOp::Gt,
        Token::GtEq => BinaryOp::GtEq,
        Token::Concat => BinaryOp::Concat,
        Token::Keyword(Keyword::And) => BinaryOp::And,
        Token::Keyword(Keyword::Or) => BinaryOp::Or,
        _ => return None,
    };
    Some(op)
}

struct Parser {
    tokens: Vec<PositionedToken>,
    i: usize,
    end: Position,
}

impl Parser {
    fn statement(&mut self) -> Result<Statement> {
        if self.eat_keyword(Keyword::Select) {
            return Ok(Statement::Select(self.select()?));
        }
        match self.peek() {
            // Statements whose slices have not landed yet.
            Some(Token::Keyword(
                Keyword::Create
                | Keyword::Drop
                | Keyword::Insert
                | Keyword::Update
                | Keyword::Delete,
            )) => Err(DbError::NotYetSupported),
            _ => Err(self.error("expected a statement")),
        }
    }

    /// The part of a SELECT after the `SELECT` keyword.
    fn select(&mut self) -> Result<Select> {
        let mut items = vec![self.select_item()?];
        while self.eat(&Token::Comma) {
            items.push(self.select_item()?);
        }
        Ok(Select { items })
    }

    fn select_item(&mut self) -> Result<SelectItem> {
        let expr = self.expr()?;
        let alias = if self.eat_keyword(Keyword::As) {
            Some(self.ident()?)
        } else {
            None
        };
        Ok(SelectItem { expr, alias })
    }

    fn ident(&mut self) -> Result<String> {
        match self.peek() {
            Some(Token::Ident(name)) => {
                let name = name.clone();
                self.i += 1;
                Ok(name)
            }
            _ => Err(self.error("expected a name")),
        }
    }

    fn expr(&mut self) -> Result<Expr> {
        self.expr_binding(0)
    }

    /// Parses an expression whose operators all bind at least as tightly as
    /// `min_power`. A looser operator ends it and is left for the caller.
    fn expr_binding(&mut self, min_power: u8) -> Result<Expr> {
        let mut left = self.prefix()?;
        loop {
            if self.peek() == Some(&Token::Keyword(Keyword::Is)) {
                if power::IS < min_power {
                    break;
                }
                left = self.is_null(left)?;
                continue;
            }
            let Some(op) = self.peek().and_then(binary_op) else {
                break;
            };
            let power = power::of(op);
            if power < min_power {
                break;
            }
            self.i += 1;
            // `+ 1` makes operators of equal power left-associative.
            let right = self.expr_binding(power + 1)?;
            left = Expr::Binary {
                op,
                left: Box::new(left),
                right: Box::new(right),
            };
        }
        Ok(left)
    }

    /// `IS [NOT] NULL`, applied to the expression before it.
    fn is_null(&mut self, operand: Expr) -> Result<Expr> {
        self.i += 1; // IS
        let negated = self.eat_keyword(Keyword::Not);
        if !self.eat_keyword(Keyword::Null) {
            return Err(self.error("expected NULL"));
        }
        Ok(Expr::IsNull {
            operand: Box::new(operand),
            negated,
        })
    }

    /// A unary operator applied to what follows, or a primary expression.
    fn prefix(&mut self) -> Result<Expr> {
        let (op, power) = if self.eat(&Token::Minus) {
            (UnaryOp::Neg, power::NEG)
        } else if self.eat_keyword(Keyword::Not) {
            (UnaryOp::Not, power::NOT)
        } else {
            return self.primary();
        };
        let operand = self.expr_binding(power)?;
        Ok(Expr::Unary {
            op,
            operand: Box::new(operand),
        })
    }

    /// A literal, a Column name, or a parenthesised expression.
    fn primary(&mut self) -> Result<Expr> {
        if self.eat(&Token::LParen) {
            let expr = self.expr()?;
            if !self.eat(&Token::RParen) {
                return Err(self.error("expected )"));
            }
            return Ok(expr);
        }
        let expr = match self.peek() {
            Some(Token::Integer(i)) => Expr::Literal(Value::Int(*i)),
            Some(Token::Float(x)) => Expr::Literal(Value::Float(*x)),
            Some(Token::Text(s)) => Expr::Literal(Value::Text(s.clone())),
            Some(Token::Keyword(Keyword::True)) => Expr::Literal(Value::Bool(true)),
            Some(Token::Keyword(Keyword::False)) => Expr::Literal(Value::Bool(false)),
            Some(Token::Keyword(Keyword::Null)) => Expr::Literal(Value::Null),
            Some(Token::Ident(name)) => Expr::Column(name.clone()),
            _ => return Err(self.error("expected an expression")),
        };
        self.i += 1;
        Ok(expr)
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.i).map(|t| &t.token)
    }

    /// Where the next token starts, or the end of input if there is none.
    fn pos(&self) -> Position {
        self.tokens.get(self.i).map_or(self.end, |t| t.pos)
    }

    fn eat(&mut self, expected: &Token) -> bool {
        if self.peek() == Some(expected) {
            self.i += 1;
            true
        } else {
            false
        }
    }

    fn eat_keyword(&mut self, keyword: Keyword) -> bool {
        self.eat(&Token::Keyword(keyword))
    }

    fn error(&self, message: &str) -> DbError {
        DbError::Syntax {
            pos: self.pos(),
            message: message.to_string(),
        }
    }
}
