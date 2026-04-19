use std::fmt;

use crate::token::Token;

#[derive(Debug, Clone, PartialEq)]
pub struct Program<'a> {
    begin_blocks: Vec<Action<'a>>,
    rules: Vec<Rule<'a>>,
    end_blocks: Vec<Action<'a>>,
    function_definitions: Vec<FunctionDefinition<'a>>,
}

impl<'a> Program<'a> {
    pub fn new() -> Self {
        Program {
            begin_blocks: vec![],
            rules: vec![],
            end_blocks: vec![],
            function_definitions: vec![],
        }
    }

    pub fn len(&self) -> usize {
        self.rules.len() + self.begin_blocks.len() + self.end_blocks.len()
    }

    pub fn add_begin_block(&mut self, action: Action<'a>) {
        self.begin_blocks.push(action);
    }

    pub fn add_end_block(&mut self, action: Action<'a>) {
        self.end_blocks.push(action);
    }

    pub fn add_rule(&mut self, rule: Rule<'a>) {
        self.rules.push(rule);
    }

    pub fn add_function_definition(&mut self, definition: FunctionDefinition<'a>) {
        self.function_definitions.push(definition);
    }

    pub fn begin_blocks_iter(&self) -> std::slice::Iter<'_, Action<'a>> {
        self.begin_blocks.iter()
    }

    pub fn end_blocks_iter(&self) -> std::slice::Iter<'_, Action<'a>> {
        self.end_blocks.iter()
    }

    pub fn rules_iter(&self) -> std::slice::Iter<'_, Rule<'a>> {
        self.rules.iter()
    }

    pub fn function_definition(&self, name: &str) -> Option<&FunctionDefinition<'a>> {
        self.function_definitions
            .iter()
            .find(|definition| definition.name == name)
    }
}

impl<'a> Default for Program<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> fmt::Display for Program<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for action in &self.begin_blocks {
            write!(f, "BEGIN {action}")?;
        }

        // Add space between begin blocks and rules if both exist
        if !self.begin_blocks.is_empty() && !self.rules.is_empty() {
            write!(f, " ")?;
        }

        for rule in &self.rules {
            write!(f, "{rule}")?;
        }

        // Add space between rules and end blocks if both exist
        if !self.rules.is_empty() && !self.end_blocks.is_empty() {
            write!(f, " ")?;
        }

        for action in &self.end_blocks {
            write!(f, "END {action}")?;
        }

        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Statement<'a> {
    Empty,
    Expression(Expression<'a>),
    Print(Vec<Expression<'a>>),
    PrintRedirect {
        expressions: Vec<Expression<'a>>,
        target: Expression<'a>,
        append: bool,
    },
    PrintPipe {
        expressions: Vec<Expression<'a>>,
        target: Expression<'a>,
    },
    Printf(Vec<Expression<'a>>),
    System(Expression<'a>),
    Split {
        string: Expression<'a>,
        array: &'a str,
        separator: Option<Expression<'a>>,
    },
    Sub {
        pattern: Expression<'a>,
        replacement: Expression<'a>,
        target: Option<Expression<'a>>,
    },
    Gsub {
        pattern: Expression<'a>,
        replacement: Expression<'a>,
        target: Option<Expression<'a>>,
    },
    Assignment {
        identifier: &'a str,
        value: Expression<'a>,
    },
    SplitAssignment {
        identifier: &'a str,
        string: Expression<'a>,
        array: &'a str,
        separator: Option<Expression<'a>>,
    },
    ArrayAssignment {
        identifier: &'a str,
        index: Expression<'a>,
        value: Expression<'a>,
    },
    FieldAssignment {
        field: Expression<'a>,
        value: Expression<'a>,
    },
    AddAssignment {
        identifier: &'a str,
        value: Expression<'a>,
    },
    ArrayAddAssignment {
        identifier: &'a str,
        index: Expression<'a>,
        value: Expression<'a>,
    },
    ArrayPostIncrement {
        identifier: &'a str,
        index: Expression<'a>,
    },
    ArrayPostDecrement {
        identifier: &'a str,
        index: Expression<'a>,
    },
    Delete {
        identifier: &'a str,
        index: Option<Expression<'a>>,
    },
    PreIncrement {
        identifier: &'a str,
    },
    PreDecrement {
        identifier: &'a str,
    },
    If {
        condition: Expression<'a>,
        then_statements: Vec<Statement<'a>>,
    },
    IfElse {
        condition: Expression<'a>,
        then_statements: Vec<Statement<'a>>,
        else_statements: Vec<Statement<'a>>,
    },
    While {
        condition: Expression<'a>,
        statements: Vec<Statement<'a>>,
    },
    DoWhile {
        condition: Expression<'a>,
        statements: Vec<Statement<'a>>,
    },
    For {
        init: Box<Statement<'a>>,
        condition: Expression<'a>,
        update: Box<Statement<'a>>,
        statements: Vec<Statement<'a>>,
    },
    ForIn {
        variable: &'a str,
        array: &'a str,
        statements: Vec<Statement<'a>>,
    },
    Break,
    Continue,
    Return(Option<Expression<'a>>),
    Next,
    Exit(Option<Expression<'a>>),
    PostIncrement {
        identifier: &'a str,
    },
    PostDecrement {
        identifier: &'a str,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action<'a> {
    pub statements: Vec<Statement<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FunctionDefinition<'a> {
    pub name: &'a str,
    pub parameters: Vec<&'a str>,
    pub statements: Vec<Statement<'a>>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Rule<'a> {
    Begin(Action<'a>),
    Action(Action<'a>),
    PatternAction {
        pattern: Option<Expression<'a>>,
        action: Option<Action<'a>>,
    },
    End(Action<'a>),
}

impl<'a> fmt::Display for Rule<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Rule::Begin(action) => write!(f, "BEGIN {}", action),
            Rule::Action(action) => write!(f, "{}", action),
            Rule::PatternAction { pattern, action } => match (pattern, action) {
                (Some(expr), Some(action)) => write!(f, "{} {}", expr, action),
                (Some(expr), None) => write!(f, "{}", expr),
                (None, Some(action)) => write!(f, "{}", action),
                (None, None) => write!(f, ""),
            },
            Rule::End(action) => write!(f, "END {}", action),
        }
    }
}

impl<'a> fmt::Display for Action<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.statements.is_empty() {
            write!(f, "{{}}")
        } else {
            write!(
                f,
                "{{ {} }}",
                self.statements
                    .iter()
                    .map(|stmt| stmt.to_string())
                    .collect::<Vec<String>>()
                    .join("; ")
            )
        }
    }
}

impl<'a> fmt::Display for Statement<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Statement::Empty => write!(f, ""),
            Statement::Expression(expression) => write!(f, "{expression}"),
            Statement::Print(expressions) => {
                if expressions.is_empty() {
                    write!(f, "print")
                } else {
                    write!(
                        f,
                        "print {}",
                        expressions
                            .iter()
                            .filter(|expr| *expr != &Expression::String(" "))
                            .map(|expr| expr.to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                }
            }
            Statement::PrintRedirect {
                expressions,
                target,
                append,
            } => {
                let operator = if *append { ">>" } else { ">" };
                if expressions.is_empty() {
                    write!(f, "print {operator} {target}")
                } else {
                    write!(
                        f,
                        "print {} {operator} {target}",
                        expressions
                            .iter()
                            .map(|expr| expr.to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                }
            }
            Statement::PrintPipe {
                expressions,
                target,
            } => {
                if expressions.is_empty() {
                    write!(f, "print | {target}")
                } else {
                    write!(
                        f,
                        "print {} | {target}",
                        expressions
                            .iter()
                            .map(|expr| expr.to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                }
            }
            Statement::Printf(expressions) => {
                if expressions.is_empty() {
                    write!(f, "printf")
                } else {
                    write!(
                        f,
                        "printf {}",
                        expressions
                            .iter()
                            .map(|expr| expr.to_string())
                            .collect::<Vec<String>>()
                            .join(", ")
                    )
                }
            }
            Statement::System(command) => write!(f, "system({command})"),
            Statement::Split {
                string,
                array,
                separator,
            } => {
                write!(f, "split({string}, {array}")?;
                if let Some(separator) = separator {
                    write!(f, ", {separator}")?;
                }
                write!(f, ")")
            }
            Statement::Sub {
                pattern,
                replacement,
                target,
            } => {
                write!(f, "sub({}, {}", pattern, replacement)?;
                if let Some(target) = target {
                    write!(f, ", {target}")?;
                }
                write!(f, ")")
            }
            Statement::Gsub {
                pattern,
                replacement,
                target,
            } => {
                write!(f, "gsub({}, {}", pattern, replacement)?;
                if let Some(target) = target {
                    write!(f, ", {target}")?;
                }
                write!(f, ")")
            }
            Statement::Assignment { identifier, value } => write!(f, "{identifier} = {value}"),
            Statement::SplitAssignment {
                identifier,
                string,
                array,
                separator,
            } => {
                write!(f, "{identifier} = split({string}, {array}")?;
                if let Some(separator) = separator {
                    write!(f, ", {separator}")?;
                }
                write!(f, ")")
            }
            Statement::ArrayAssignment {
                identifier,
                index,
                value,
            } => write!(f, "{identifier}[{index}] = {value}"),
            Statement::FieldAssignment { field, value } => write!(f, "${field} = {value}"),
            Statement::AddAssignment { identifier, value } => {
                write!(f, "{identifier} += {value}")
            }
            Statement::ArrayAddAssignment {
                identifier,
                index,
                value,
            } => write!(f, "{identifier}[{index}] += {value}"),
            Statement::ArrayPostIncrement { identifier, index } => {
                write!(f, "{identifier}[{index}]++")
            }
            Statement::ArrayPostDecrement { identifier, index } => {
                write!(f, "{identifier}[{index}]--")
            }
            Statement::Delete { identifier, index } => {
                if let Some(index) = index {
                    write!(f, "delete {identifier}[{index}]")
                } else {
                    write!(f, "delete {identifier}")
                }
            }
            Statement::PreIncrement { identifier } => write!(f, "++{identifier}"),
            Statement::PreDecrement { identifier } => write!(f, "--{identifier}"),
            Statement::If {
                condition,
                then_statements,
            } => {
                let rendered = then_statements
                    .iter()
                    .map(|stmt| stmt.to_string())
                    .collect::<Vec<String>>()
                    .join("; ");
                write!(f, "if ({condition}) {{ {rendered} }}")
            }
            Statement::IfElse {
                condition,
                then_statements,
                else_statements,
            } => {
                let then_rendered = then_statements
                    .iter()
                    .map(|stmt| stmt.to_string())
                    .collect::<Vec<String>>()
                    .join("; ");
                let else_rendered = else_statements
                    .iter()
                    .map(|stmt| stmt.to_string())
                    .collect::<Vec<String>>()
                    .join("; ");
                write!(
                    f,
                    "if ({condition}) {{ {then_rendered} }} else {{ {else_rendered} }}"
                )
            }
            Statement::While {
                condition,
                statements,
            } => {
                let rendered = statements
                    .iter()
                    .map(|stmt| stmt.to_string())
                    .collect::<Vec<String>>()
                    .join("; ");
                write!(f, "while ({condition}) {{ {rendered} }}")
            }
            Statement::DoWhile {
                condition,
                statements,
            } => {
                let rendered = statements
                    .iter()
                    .map(|stmt| stmt.to_string())
                    .collect::<Vec<String>>()
                    .join("; ");
                write!(f, "do {{ {rendered} }} while ({condition})")
            }
            Statement::For {
                init,
                condition,
                update,
                statements,
            } => {
                let rendered = statements
                    .iter()
                    .map(|stmt| stmt.to_string())
                    .collect::<Vec<String>>()
                    .join("; ");
                write!(f, "for ({init}; {condition}; {update}) {{ {rendered} }}")
            }
            Statement::ForIn {
                variable,
                array,
                statements,
            } => {
                let rendered = statements
                    .iter()
                    .map(|stmt| stmt.to_string())
                    .collect::<Vec<String>>()
                    .join("; ");
                write!(f, "for ({variable} in {array}) {{ {rendered} }}")
            }
            Statement::Break => write!(f, "break"),
            Statement::Continue => write!(f, "continue"),
            Statement::Return(None) => write!(f, "return"),
            Statement::Return(Some(value)) => write!(f, "return {value}"),
            Statement::Next => write!(f, "next"),
            Statement::Exit(None) => write!(f, "exit"),
            Statement::Exit(Some(status)) => write!(f, "exit {status}"),
            Statement::PostIncrement { identifier } => write!(f, "{identifier}++"),
            Statement::PostDecrement { identifier } => write!(f, "{identifier}--"),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expression<'a> {
    Number(f64),
    HexNumber {
        literal: &'a str,
        value: f64,
    },
    String(&'a str),
    Regex(&'a str),
    Field(Box<Expression<'a>>),
    Identifier(&'a str),
    ArrayAccess {
        identifier: &'a str,
        index: Box<Expression<'a>>,
    },
    Length(Option<Box<Expression<'a>>>),
    Substr {
        string: Box<Expression<'a>>,
        start: Box<Expression<'a>>,
        length: Option<Box<Expression<'a>>>,
    },
    Rand,
    FunctionCall {
        name: &'a str,
        args: Vec<Expression<'a>>,
    },
    Not(Box<Expression<'a>>),
    PreIncrement(Box<Expression<'a>>),
    PreDecrement(Box<Expression<'a>>),
    PostIncrement(Box<Expression<'a>>),
    PostDecrement(Box<Expression<'a>>),
    Ternary {
        condition: Box<Expression<'a>>,
        then_expr: Box<Expression<'a>>,
        else_expr: Box<Expression<'a>>,
    },
    Concatenation {
        left: Box<Expression<'a>>,
        right: Box<Expression<'a>>,
    },
    // non_unary_expr
    Infix {
        left: Box<Expression<'a>>,
        operator: Token<'a>,
        right: Box<Expression<'a>>,
    },
}

impl<'a> fmt::Display for Expression<'a> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Expression::Number(n) => write!(f, "{}", n),
            Expression::HexNumber { literal, .. } => write!(f, "{literal}"),
            Expression::String(value) => write!(f, "\"{}\"", value),
            Expression::Regex(value) => write!(f, "/{}/", value),
            Expression::Field(expr) => write!(f, "${}", expr),
            Expression::Identifier(ident) => write!(f, "{}", ident),
            Expression::ArrayAccess { identifier, index } => write!(f, "{identifier}[{index}]"),
            Expression::Length(None) => write!(f, "length"),
            Expression::Length(Some(expr)) => write!(f, "length({})", expr),
            Expression::Substr {
                string,
                start,
                length,
            } => {
                if let Some(length) = length {
                    write!(f, "substr({}, {}, {})", string, start, length)
                } else {
                    write!(f, "substr({}, {})", string, start)
                }
            }
            Expression::Rand => write!(f, "rand()"),
            Expression::FunctionCall { name, args } => write!(
                f,
                "{name}({})",
                args.iter()
                    .map(|arg| arg.to_string())
                    .collect::<Vec<String>>()
                    .join(", ")
            ),
            Expression::Not(expr) => write!(f, "!{expr}"),
            Expression::PreIncrement(expr) => write!(f, "++{expr}"),
            Expression::PreDecrement(expr) => write!(f, "--{expr}"),
            Expression::PostIncrement(expr) => write!(f, "{expr}++"),
            Expression::PostDecrement(expr) => write!(f, "{expr}--"),
            Expression::Ternary {
                condition,
                then_expr,
                else_expr,
            } => write!(f, "({condition}) ? {then_expr} : {else_expr}"),
            Expression::Concatenation { left, right } => write!(f, "{} {}", left, right),
            Expression::Infix {
                left,
                operator,
                right,
            } => {
                if operator.kind == crate::token::TokenKind::Comma {
                    write!(f, "{left}, {right}")
                } else {
                    write!(f, "{} {} {}", left, operator.literal, right)
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::token::TokenKind;

    #[test]
    fn test_add_block_to_program() {
        let mut program = Program::new();

        let action = Action {
            statements: vec![Statement::Print(vec![])],
        };
        program.add_begin_block(action);

        assert_eq!(program.begin_blocks.len(), 1);
    }

    #[test]
    fn test_add_rule_to_program() {
        let mut program = Program::new();

        let rule = Rule::Action(Action {
            statements: vec![Statement::Print(vec![])],
        });
        program.add_rule(rule);

        assert_eq!(program.len(), 1);
    }

    #[test]
    fn test_program_creation() {
        let expected_string = "$3 > 5";
        let program = Program {
            begin_blocks: vec![],
            rules: vec![Rule::PatternAction {
                pattern: Some(Expression::Infix {
                    left: Box::new(Expression::Field(Box::new(Expression::Number(3.0)))),
                    operator: Token::new(TokenKind::GreaterThan, ">", 3),
                    right: Box::new(Expression::Number(5.0)),
                }),
                action: None,
            }],
            end_blocks: vec![],
            function_definitions: vec![],
        };

        assert_eq!(expected_string, program.to_string());
    }

    #[test]
    fn test_begin_block_program_creation() {
        let expected_string = "BEGIN { print }";
        let program = Program {
            begin_blocks: vec![Action {
                statements: vec![Statement::Print(vec![])],
            }],
            rules: vec![],
            end_blocks: vec![],
            function_definitions: vec![],
        };

        assert!(program.len() == 1);
        assert_eq!(expected_string, program.to_string());
    }

    #[test]
    fn test_end_block_program_creation() {
        let expected_string = "END { print }";
        let program = Program {
            begin_blocks: vec![],
            rules: vec![],
            end_blocks: vec![Action {
                statements: vec![Statement::Print(vec![])],
            }],
            function_definitions: vec![],
        };

        assert!(program.len() == 1);
        assert_eq!(expected_string, program.to_string());
    }

    #[test]
    fn test_begin_rule_display() {
        let rule = Rule::Begin(Action {
            statements: vec![Statement::Print(vec![])],
        });

        assert_eq!("BEGIN { print }", rule.to_string());
    }

    #[test]
    fn test_end_rule_display() {
        let rule = Rule::End(Action {
            statements: vec![Statement::Print(vec![])],
        });

        assert_eq!("END { print }", rule.to_string());
    }

    #[test]
    fn test_action_without_pattern_program_creation() {
        let expected_string = "{ print }";
        let program = Program {
            begin_blocks: vec![],
            rules: vec![Rule::PatternAction {
                pattern: None,
                action: Some(Action {
                    statements: vec![Statement::Print(vec![])],
                }),
            }],
            end_blocks: vec![],
            function_definitions: vec![],
        };

        assert!(program.len() == 1);
        assert_eq!(expected_string, program.to_string());
    }

    #[test]
    fn test_program_with_begin_body_and_end_blocks() {
        let expected_string =
            "BEGIN { print } $1 == 42 { print NF, $2, $3 } END { print \"hello\" }";
        let program = Program {
            begin_blocks: vec![Action {
                statements: vec![Statement::Print(vec![])],
            }],
            rules: vec![Rule::PatternAction {
                pattern: Some(Expression::Infix {
                    left: Box::new(Expression::Field(Box::new(Expression::Number(1.0)))),
                    operator: Token::new(TokenKind::Equal, "==", 7),
                    right: Box::new(Expression::Number(42.0)),
                }),
                action: Some(Action {
                    statements: vec![Statement::Print(vec![
                        Expression::Identifier("NF"),
                        Expression::String(" "),
                        Expression::Field(Box::new(Expression::Number(2.0))),
                        Expression::String(" "),
                        Expression::Field(Box::new(Expression::Number(3.0))),
                    ])],
                }),
            }],
            end_blocks: vec![Action {
                statements: vec![Statement::Print(vec![Expression::String("hello".into())])],
            }],
            function_definitions: vec![],
        };

        assert_eq!(expected_string, program.to_string());
    }

    #[test]
    fn test_print_regex_expression() {
        let expr = Expression::Regex("^[a-z]+$");

        assert_eq!("/^[a-z]+$/", expr.to_string());
    }

    #[test]
    fn test_assignment_statement_display() {
        let statement = Statement::Assignment {
            identifier: "pop",
            value: Expression::Infix {
                left: Box::new(Expression::Identifier("pop")),
                operator: Token::new(TokenKind::Plus, "+", 0),
                right: Box::new(Expression::Field(Box::new(Expression::Number(3.0)))),
            },
        };

        assert_eq!("pop = pop + $3", statement.to_string());
    }

    #[test]
    fn test_add_assignment_statement_display() {
        let statement = Statement::AddAssignment {
            identifier: "pop",
            value: Expression::Field(Box::new(Expression::Number(3.0))),
        };

        assert_eq!("pop += $3", statement.to_string());
    }

    #[test]
    fn test_pre_increment_statement_display() {
        let statement = Statement::PreIncrement { identifier: "n" };

        assert_eq!("++n", statement.to_string());
    }

    #[test]
    fn test_pre_decrement_statement_display() {
        let statement = Statement::PreDecrement { identifier: "n" };

        assert_eq!("--n", statement.to_string());
    }

    #[test]
    fn test_action_with_new_statements_display() {
        let action = Action {
            statements: vec![
                Statement::AddAssignment {
                    identifier: "pop",
                    value: Expression::Field(Box::new(Expression::Number(3.0))),
                },
                Statement::PreIncrement { identifier: "n" },
            ],
        };

        assert_eq!("{ pop += $3; ++n }", action.to_string());
    }

    #[test]
    fn test_gsub_statement_display() {
        let statement = Statement::Gsub {
            pattern: Expression::Regex("USA"),
            replacement: Expression::String("United States"),
            target: None,
        };

        assert_eq!(r#"gsub(/USA/, "United States")"#, statement.to_string());
    }

    #[test]
    fn test_system_statement_display() {
        let statement = Statement::System(Expression::Concatenation {
            left: Box::new(Expression::String("cat ")),
            right: Box::new(Expression::Field(Box::new(Expression::Number(2.0)))),
        });

        assert_eq!(r#"system("cat " $2)"#, statement.to_string());
    }

    #[test]
    fn test_length_expression_without_argument_display() {
        let expression = Expression::Length(None);

        assert_eq!("length", expression.to_string());
    }

    #[test]
    fn test_length_expression_with_argument_display() {
        let expression = Expression::Length(Some(Box::new(Expression::Field(Box::new(
            Expression::Number(1.0),
        )))));

        assert_eq!("length($1)", expression.to_string());
    }

    #[test]
    fn test_print_statement_with_length_expression_display() {
        let statement = Statement::Print(vec![
            Expression::Length(None),
            Expression::String(" "),
            Expression::Field(Box::new(Expression::Number(0.0))),
        ]);

        assert_eq!("print length, $0", statement.to_string());
    }

    #[test]
    fn test_substr_expression_display() {
        let expression = Expression::Substr {
            string: Box::new(Expression::Field(Box::new(Expression::Number(1.0)))),
            start: Box::new(Expression::Number(1.0)),
            length: Some(Box::new(Expression::Number(3.0))),
        };

        assert_eq!("substr($1, 1, 3)", expression.to_string());
    }

    #[test]
    fn test_field_assignment_statement_display() {
        let statement = Statement::FieldAssignment {
            field: Expression::Number(1.0),
            value: Expression::Substr {
                string: Box::new(Expression::Field(Box::new(Expression::Number(1.0)))),
                start: Box::new(Expression::Number(1.0)),
                length: Some(Box::new(Expression::Number(3.0))),
            },
        };

        assert_eq!("$1 = substr($1, 1, 3)", statement.to_string());
    }

    #[test]
    fn test_concatenation_expression_display() {
        let expression = Expression::Concatenation {
            left: Box::new(Expression::Identifier("s")),
            right: Box::new(Expression::Substr {
                string: Box::new(Expression::Field(Box::new(Expression::Number(1.0)))),
                start: Box::new(Expression::Number(1.0)),
                length: Some(Box::new(Expression::Number(3.0))),
            }),
        };

        assert_eq!("s substr($1, 1, 3)", expression.to_string());
    }

    #[test]
    fn test_expression_display_for_not_increment_decrement_and_ternary() {
        let statement = Statement::Print(vec![
            Expression::Not(Box::new(Expression::Identifier("x"))),
            Expression::PreIncrement(Box::new(Expression::Identifier("x"))),
            Expression::PreDecrement(Box::new(Expression::Identifier("x"))),
            Expression::PostIncrement(Box::new(Expression::Identifier("x"))),
            Expression::PostDecrement(Box::new(Expression::Identifier("x"))),
            Expression::Ternary {
                condition: Box::new(Expression::Identifier("x")),
                then_expr: Box::new(Expression::Identifier("y")),
                else_expr: Box::new(Expression::Identifier("z")),
            },
        ]);

        assert_eq!(
            "print !x, ++x, --x, x++, x--, (x) ? y : z",
            statement.to_string()
        );
    }

    #[test]
    fn test_if_statement_display() {
        let statement = Statement::If {
            condition: Expression::Infix {
                left: Box::new(Expression::Identifier("maxpop")),
                operator: Token::new(TokenKind::LessThan, "<", 0),
                right: Box::new(Expression::Field(Box::new(Expression::Number(3.0)))),
            },
            then_statements: vec![Statement::Assignment {
                identifier: "maxpop",
                value: Expression::Field(Box::new(Expression::Number(3.0))),
            }],
        };

        assert_eq!("if (maxpop < $3) { maxpop = $3 }", statement.to_string());
    }

    #[test]
    fn test_while_statement_display() {
        let statement = Statement::While {
            condition: Expression::Infix {
                left: Box::new(Expression::Identifier("i")),
                operator: Token::new(TokenKind::LessThanOrEqual, "<=", 0),
                right: Box::new(Expression::Identifier("NF")),
            },
            statements: vec![
                Statement::Print(vec![Expression::Field(Box::new(Expression::Identifier(
                    "i",
                )))]),
                Statement::PostIncrement { identifier: "i" },
            ],
        };

        assert_eq!("while (i <= NF) { print $i; i++ }", statement.to_string());
    }

    #[test]
    fn test_do_while_statement_display() {
        let statement = Statement::DoWhile {
            condition: Expression::Infix {
                left: Box::new(Expression::Identifier("i")),
                operator: Token::new(TokenKind::LessThanOrEqual, "<=", 0),
                right: Box::new(Expression::Identifier("NF")),
            },
            statements: vec![
                Statement::Print(vec![Expression::Field(Box::new(Expression::Identifier(
                    "i",
                )))]),
                Statement::PostIncrement { identifier: "i" },
            ],
        };

        assert_eq!(
            "do { print $i; i++ } while (i <= NF)",
            statement.to_string()
        );
    }

    #[test]
    fn test_for_statement_display() {
        let statement = Statement::For {
            init: Box::new(Statement::Assignment {
                identifier: "i",
                value: Expression::Number(1.0),
            }),
            condition: Expression::Infix {
                left: Box::new(Expression::Identifier("i")),
                operator: Token::new(TokenKind::LessThanOrEqual, "<=", 0),
                right: Box::new(Expression::Identifier("NF")),
            },
            update: Box::new(Statement::PostIncrement { identifier: "i" }),
            statements: vec![Statement::Print(vec![Expression::Field(Box::new(
                Expression::Identifier("i"),
            ))])],
        };

        assert_eq!(
            "for (i = 1; i <= NF; i++) { print $i }",
            statement.to_string()
        );
    }

    #[test]
    fn test_post_decrement_statement_display() {
        let statement = Statement::PostDecrement { identifier: "n" };

        assert_eq!("n--", statement.to_string());
    }

    #[test]
    fn test_rand_expression_display() {
        let expression = Expression::Rand;

        assert_eq!("rand()", expression.to_string());
    }

    #[test]
    fn test_exit_statement_display() {
        let statement = Statement::Exit(None);

        assert_eq!("exit", statement.to_string());
    }

    #[test]
    fn test_exit_statement_with_status_display() {
        let statement = Statement::Exit(Some(Expression::Identifier("NR")));

        assert_eq!("exit NR", statement.to_string());
    }

    #[test]
    fn test_print_redirect_statement_display() {
        let statement = Statement::PrintRedirect {
            expressions: vec![],
            target: Expression::String("tempbig"),
            append: false,
        };

        assert_eq!(r#"print > "tempbig""#, statement.to_string());
    }

    #[test]
    fn test_print_pipe_statement_display() {
        let statement = Statement::PrintPipe {
            expressions: vec![
                Expression::Identifier("c"),
                Expression::String(":"),
                Expression::Number(1.0),
            ],
            target: Expression::String("sort"),
        };

        assert_eq!(r#"print c, ":", 1 | "sort""#, statement.to_string());
    }

    #[test]
    fn test_for_in_statement_display() {
        let statement = Statement::ForIn {
            variable: "name",
            array: "area",
            statements: vec![Statement::Print(vec![Expression::Concatenation {
                left: Box::new(Expression::Identifier("name")),
                right: Box::new(Expression::ArrayAccess {
                    identifier: "area",
                    index: Box::new(Expression::Identifier("name")),
                }),
            }])],
        };

        assert_eq!(
            "for (name in area) { print name area[name] }",
            statement.to_string()
        );
    }

    #[test]
    fn test_array_access_expression_display() {
        let expression = Expression::ArrayAccess {
            identifier: "pop",
            index: Box::new(Expression::String("Asia")),
        };

        assert_eq!(r#"pop["Asia"]"#, expression.to_string());
    }

    #[test]
    fn test_array_add_assignment_display() {
        let statement = Statement::ArrayAddAssignment {
            identifier: "pop",
            index: Expression::String("Asia"),
            value: Expression::Field(Box::new(Expression::Number(3.0))),
        };

        assert_eq!(r#"pop["Asia"] += $3"#, statement.to_string());
    }
}
