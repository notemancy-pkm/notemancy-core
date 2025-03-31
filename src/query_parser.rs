// src/query_parser.rs

use anyhow::{Result, anyhow};
use std::iter::Peekable;

#[derive(Debug, PartialEq, Clone)]
pub enum Token {
    Identifier(String),
    Operator(String),
    StringLiteral(String),
    And,
    Or,
    Not,
    LParen,
    RParen,
}

fn is_operator_char(c: char) -> bool {
    matches!(c, '=' | '!' | '>' | '<')
}

/// Tokenizes the input DSL query string.
pub fn tokenize(input: &str) -> Result<Vec<Token>> {
    let mut tokens = Vec::new();
    let mut chars = input.chars().peekable();

    while let Some(&c) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
        } else if c == '(' {
            tokens.push(Token::LParen);
            chars.next();
        } else if c == ')' {
            tokens.push(Token::RParen);
            chars.next();
        } else if c == '"' {
            chars.next(); // skip opening quote
            let mut literal = String::new();
            while let Some(&ch) = chars.peek() {
                if ch == '"' {
                    chars.next(); // skip closing quote
                    break;
                } else {
                    literal.push(ch);
                    chars.next();
                }
            }
            tokens.push(Token::StringLiteral(literal));
        } else if is_operator_char(c) {
            let mut op = String::new();
            op.push(c);
            chars.next();
            if let Some(&next_ch) = chars.peek() {
                if is_operator_char(next_ch) {
                    op.push(next_ch);
                    chars.next();
                }
            }
            tokens.push(Token::Operator(op));
        } else {
            let mut ident = String::new();
            while let Some(&ch) = chars.peek() {
                if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                    ident.push(ch);
                    chars.next();
                } else {
                    break;
                }
            }
            match ident.to_lowercase().as_str() {
                "and" => tokens.push(Token::And),
                "or" => tokens.push(Token::Or),
                "not" => tokens.push(Token::Not),
                _ => tokens.push(Token::Identifier(ident)),
            }
        }
    }
    Ok(tokens)
}

#[derive(Debug)]
pub enum Expr {
    Condition {
        field: String,
        op: String,
        value: String,
    },
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
}

struct Parser<I: Iterator<Item = Token>> {
    tokens: Peekable<I>,
}

impl<I: Iterator<Item = Token>> Parser<I> {
    fn new(tokens: I) -> Self {
        Parser {
            tokens: tokens.peekable(),
        }
    }

    fn parse_expression(&mut self) -> Result<Expr> {
        self.parse_or()
    }

    fn parse_or(&mut self) -> Result<Expr> {
        let mut expr = self.parse_and()?;
        while let Some(Token::Or) = self.tokens.peek() {
            self.tokens.next(); // consume 'or'
            let rhs = self.parse_and()?;
            expr = Expr::Or(Box::new(expr), Box::new(rhs));
        }
        Ok(expr)
    }

    fn parse_and(&mut self) -> Result<Expr> {
        let mut expr = self.parse_unary()?;
        while let Some(Token::And) = self.tokens.peek() {
            self.tokens.next(); // consume 'and'
            let rhs = self.parse_unary()?;
            expr = Expr::And(Box::new(expr), Box::new(rhs));
        }
        Ok(expr)
    }

    fn parse_unary(&mut self) -> Result<Expr> {
        if let Some(Token::Not) = self.tokens.peek() {
            self.tokens.next();
            let expr = self.parse_unary()?;
            Ok(Expr::Not(Box::new(expr)))
        } else {
            self.parse_primary()
        }
    }

    fn parse_primary(&mut self) -> Result<Expr> {
        if let Some(Token::LParen) = self.tokens.peek() {
            self.tokens.next(); // consume '('
            let expr = self.parse_expression()?;
            match self.tokens.next() {
                Some(Token::RParen) => Ok(expr),
                _ => Err(anyhow!("Expected closing parenthesis")),
            }
        } else {
            self.parse_condition()
        }
    }

    fn parse_condition(&mut self) -> Result<Expr> {
        let field = match self.tokens.next() {
            Some(Token::Identifier(s)) => s,
            other => return Err(anyhow!("Expected field name, found {:?}", other)),
        };
        let op = match self.tokens.next() {
            Some(Token::Operator(s)) => s,
            other => return Err(anyhow!("Expected operator, found {:?}", other)),
        };
        let value = match self.tokens.next() {
            Some(Token::Identifier(s)) => s,
            Some(Token::StringLiteral(s)) => s,
            other => return Err(anyhow!("Expected value, found {:?}", other)),
        };
        Ok(Expr::Condition { field, op, value })
    }
}

/// Parses a DSL query string into an abstract syntax tree (AST).
pub fn parse_query(input: &str) -> Result<Expr> {
    let tokens = tokenize(input)?;
    let mut parser = Parser::new(tokens.into_iter());
    let expr = parser.parse_expression()?;
    Ok(expr)
}

/// Converts the AST into a jq expression string.
/// For tag conditions:
///   - `tag = "CLI"` becomes `(.tags | index("CLI") != null)`
///   - `tag != "CLI"` becomes `(.tags | index("CLI") == null)`
/// For other fields, a simple comparison is generated.
/// For negation, we now output the expression as: `({inner} | not)`
pub fn build_jq_expression(expr: &Expr) -> String {
    match expr {
        Expr::Condition { field, op, value } => {
            if field.to_lowercase() == "tag" {
                if op == "=" {
                    format!("(.tags | index(\"{}\") != null)", value)
                } else if op == "!=" {
                    format!("(.tags | index(\"{}\") == null)", value)
                } else {
                    format!(".{} {} \"{}\"", field, op, value)
                }
            } else {
                format!(".{} {} \"{}\"", field, op, value)
            }
        }
        Expr::And(lhs, rhs) => {
            format!(
                "({} and {})",
                build_jq_expression(lhs),
                build_jq_expression(rhs)
            )
        }
        Expr::Or(lhs, rhs) => {
            format!(
                "({} or {})",
                build_jq_expression(lhs),
                build_jq_expression(rhs)
            )
        }
        Expr::Not(inner) => {
            // Special-case negated tag condition.
            if let Expr::Condition { field, op, value } = &**inner {
                if field.to_lowercase() == "tag" && op == "=" {
                    format!("(.tags | index(\"{}\") == null)", value)
                } else {
                    format!("({} | not)", build_jq_expression(inner))
                }
            } else {
                format!("({} | not)", build_jq_expression(inner))
            }
        }
    }
}
