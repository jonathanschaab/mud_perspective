use crate::parser::process_unicode_escape;

/// A value evaluated within a conditional statement.
#[derive(Debug, Clone, PartialEq)]
pub enum ConditionValue {
    /// A static string literal (e.g., `"raining"`).
    Literal(String),
    /// A numerical float parsed for inequalities.
    Number(f64),
    /// A dynamic context variable (e.g., `$weather`).
    Variable(String),
    /// A dynamic entity property (e.g., `source.color`).
    EntityProperty(String, String),
}

/// A logical condition that can be evaluated dynamically at render time.
#[derive(Debug, Clone, PartialEq)]
pub enum Condition {
    /// Evaluates a single value for truthiness.
    Value(ConditionValue),
    /// Inverts the boolean result of an expression.
    Not(Box<Condition>),
    /// Evaluates true if both expressions are true.
    And(Box<Condition>, Box<Condition>),
    /// Evaluates true if either expression is true.
    Or(Box<Condition>, Box<Condition>),
    /// Evaluates equality between two condition values.
    Eq(ConditionValue, ConditionValue),
    /// Evaluates inequality between two condition values.
    NotEq(ConditionValue, ConditionValue),
    /// Evaluates if the left value is strictly greater than the right value (numerically).
    Gt(ConditionValue, ConditionValue),
    /// Evaluates if the left value is strictly less than the right value (numerically).
    Lt(ConditionValue, ConditionValue),
    /// Evaluates if the left value is greater than or equal to the right value (numerically).
    GtEq(ConditionValue, ConditionValue),
    /// Evaluates if the left value is less than or equal to the right value (numerically).
    LtEq(ConditionValue, ConditionValue),
}

impl Condition {
    pub(crate) fn parse(s: &str, max_depth: usize) -> Result<Self, String> {
        let tokens = tokenize_expr(s)?;
        ExprParser::parse(&tokens, max_depth)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum ExprToken {
    And,
    Or,
    Not,
    LParen,
    RParen,
    Eq,
    NotEq,
    Gt,
    Lt,
    GtEq,
    LtEq,
    Val(ConditionValue),
}

#[allow(clippy::too_many_lines)]
fn tokenize_expr(s: &str) -> Result<Vec<ExprToken>, String> {
    let mut tokens = Vec::new();
    let mut chars = s.char_indices().peekable();

    while let Some(&(i, c)) = chars.peek() {
        if c.is_whitespace() {
            chars.next();
            continue;
        }
        match c {
            '(' => {
                tokens.push(ExprToken::LParen);
                chars.next();
            }
            ')' => {
                tokens.push(ExprToken::RParen);
                chars.next();
            }
            '=' => {
                chars.next();
                if chars.next_if(|&(_, n)| n == '=').is_some() {
                    tokens.push(ExprToken::Eq);
                } else {
                    return Err(format!("Expected '==' at index {i}"));
                }
            }
            '!' => {
                chars.next();
                if chars.next_if(|&(_, n)| n == '=').is_some() {
                    tokens.push(ExprToken::NotEq);
                } else {
                    tokens.push(ExprToken::Not);
                }
            }
            '>' => {
                chars.next();
                if chars.next_if(|&(_, n)| n == '=').is_some() {
                    tokens.push(ExprToken::GtEq);
                } else {
                    tokens.push(ExprToken::Gt);
                }
            }
            '<' => {
                chars.next();
                if chars.next_if(|&(_, n)| n == '=').is_some() {
                    tokens.push(ExprToken::LtEq);
                } else {
                    tokens.push(ExprToken::Lt);
                }
            }
            '&' => {
                chars.next();
                if chars.next_if(|&(_, n)| n == '&').is_some() {
                    tokens.push(ExprToken::And);
                } else {
                    return Err(format!("Expected '&&' at index {i}"));
                }
            }
            '|' => {
                chars.next();
                if chars.next_if(|&(_, n)| n == '|').is_some() {
                    tokens.push(ExprToken::Or);
                } else {
                    return Err(format!("Expected '||' at index {i}"));
                }
            }
            '`' => {
                chars.next();
                let mut s = String::new();
                while let Some(&(_, n)) = chars.peek() {
                    if n == '`' {
                        chars.next();
                        break;
                    }
                    s.push(n);
                    chars.next();
                }
                tokens.push(ExprToken::Val(ConditionValue::Literal(s)));
            }
            '"' | '\'' => {
                let quote = c;
                chars.next();
                let mut s = String::new();
                while let Some(&(_, n)) = chars.peek() {
                    if n == '\\' {
                        chars.next(); // Consume the '\'
                        if let Some(&(_, escaped_c)) = chars.peek() {
                            chars.next(); // Consume the escaped character
                            match escaped_c {
                                'n' => s.push('\n'),
                                'r' => s.push('\r'),
                                't' => s.push('\t'),
                                'u' => process_unicode_escape(&mut chars, &mut s),
                                _ => s.push(escaped_c), // handles \", \', and \\ naturally
                            }
                        }
                    } else if n == quote {
                        chars.next();
                        break;
                    } else {
                        s.push(n);
                        chars.next();
                    }
                }
                tokens.push(ExprToken::Val(ConditionValue::Literal(s)));
            }
            _ => {
                let mut ident = String::new();
                while let Some(&(_, n)) = chars.peek() {
                    if n.is_whitespace() || "()=!<>&|\"'".contains(n) {
                        break;
                    }
                    ident.push(n);
                    chars.next();
                }
                match ident.to_lowercase().as_str() {
                    "and" => tokens.push(ExprToken::And),
                    "or" => tokens.push(ExprToken::Or),
                    "not" => tokens.push(ExprToken::Not),
                    "true" | "false" => tokens.push(ExprToken::Val(ConditionValue::Literal(ident))),
                    _ => {
                        if let Ok(num) = ident.parse::<f64>() {
                            tokens.push(ExprToken::Val(ConditionValue::Number(num)));
                        } else if let Some(var) = ident.strip_prefix('$') {
                            tokens
                                .push(ExprToken::Val(ConditionValue::Variable(var.to_lowercase())));
                        } else if let Some((ent, prop)) = ident.rsplit_once('.') {
                            tokens.push(ExprToken::Val(ConditionValue::EntityProperty(
                                ent.to_lowercase(),
                                prop.to_string(),
                            )));
                        } else {
                            tokens.push(ExprToken::Val(ConditionValue::Literal(ident)));
                        }
                    }
                }
            }
        }
    }
    Ok(tokens)
}

struct ExprParser<'a> {
    tokens: &'a [ExprToken],
    pos: usize,
    depth: usize,
    max_depth: usize,
}

impl<'a> ExprParser<'a> {
    fn parse(tokens: &'a [ExprToken], max_depth: usize) -> Result<Condition, String> {
        let mut parser = Self {
            tokens,
            pos: 0,
            depth: 0,
            max_depth,
        };
        let expr = parser.parse_or()?;
        if parser.pos < parser.tokens.len() {
            return Err("Unexpected trailing tokens in expression".into());
        }
        Ok(expr)
    }
    fn peek(&self) -> Option<&ExprToken> {
        self.tokens.get(self.pos)
    }
    fn consume(&mut self) -> Option<&ExprToken> {
        let t = self.tokens.get(self.pos);
        self.pos += 1;
        t
    }

    fn parse_or(&mut self) -> Result<Condition, String> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(format!(
                "Maximum expression nesting depth of {} exceeded",
                self.max_depth
            ));
        }
        let mut left = self.parse_and()?;
        while let Some(ExprToken::Or) = self.peek() {
            self.consume();
            left = Condition::Or(Box::new(left), Box::new(self.parse_and()?));
        }
        self.depth -= 1;
        Ok(left)
    }
    fn parse_and(&mut self) -> Result<Condition, String> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(format!(
                "Maximum expression nesting depth of {} exceeded",
                self.max_depth
            ));
        }
        let mut left = self.parse_comparison()?;
        while let Some(ExprToken::And) = self.peek() {
            self.consume();
            left = Condition::And(Box::new(left), Box::new(self.parse_comparison()?));
        }
        self.depth -= 1;
        Ok(left)
    }
    fn parse_comparison(&mut self) -> Result<Condition, String> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(format!(
                "Maximum expression nesting depth of {} exceeded",
                self.max_depth
            ));
        }
        let left_expr = self.parse_unary()?;
        let res = match self.peek() {
            Some(
                op @ (ExprToken::Eq
                | ExprToken::NotEq
                | ExprToken::Lt
                | ExprToken::Gt
                | ExprToken::LtEq
                | ExprToken::GtEq),
            ) => {
                let op_clone = op.clone();
                self.consume();
                let Condition::Value(left_val) = left_expr else {
                    return Err("Left side of comparison must be a value".into());
                };
                let Condition::Value(right_val) = self.parse_unary()? else {
                    return Err("Right side of comparison must be a value".into());
                };
                Ok(match op_clone {
                    ExprToken::Eq => Condition::Eq(left_val, right_val),
                    ExprToken::NotEq => Condition::NotEq(left_val, right_val),
                    ExprToken::Lt => Condition::Lt(left_val, right_val),
                    ExprToken::Gt => Condition::Gt(left_val, right_val),
                    ExprToken::LtEq => Condition::LtEq(left_val, right_val),
                    ExprToken::GtEq => Condition::GtEq(left_val, right_val),
                    _ => return Err("Invalid comparison operator".into()),
                })
            }
            _ => Ok(left_expr),
        };
        self.depth -= 1;
        res
    }
    fn parse_unary(&mut self) -> Result<Condition, String> {
        self.depth += 1;
        if self.depth > self.max_depth {
            return Err(format!(
                "Maximum expression nesting depth of {} exceeded",
                self.max_depth
            ));
        }
        let res = if let Some(ExprToken::Not) = self.peek() {
            self.consume();
            Ok(Condition::Not(Box::new(self.parse_unary()?)))
        } else {
            match self.consume() {
                Some(ExprToken::Val(v)) => Ok(Condition::Value(v.clone())),
                Some(ExprToken::LParen) => {
                    let expr = self.parse_or()?;
                    if matches!(self.consume(), Some(ExprToken::RParen)) {
                        Ok(expr)
                    } else {
                        Err("Expected ')'".into())
                    }
                }
                _ => Err("Expected expression or value".into()),
            }
        };
        self.depth -= 1;
        res
    }
}
