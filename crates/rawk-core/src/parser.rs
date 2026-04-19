pub use crate::parse_error::{ParseError, ParseErrorKind};
use crate::{
    Lexer, Program,
    ast::{Action, Expression, FunctionDefinition, Rule, Statement},
    token::{Token, TokenKind},
};

#[derive(Debug)]
pub struct Parser<'a> {
    lexer: Lexer<'a>,
    current_token: Token<'a>,
    function_definitions: Vec<FunctionDefinition<'a>>,
}

impl<'a> Parser<'a> {
    pub fn new(mut lexer: Lexer<'a>) -> Self {
        let current_token = lexer.next_token_regex_aware();
        Parser {
            lexer,
            current_token,
            function_definitions: Vec::new(),
        }
    }

    fn next_token(&mut self) {
        self.current_token = self.lexer.next_token();
    }

    fn next_token_in_regex_context(&mut self) {
        self.current_token = self.lexer.next_token_regex_aware();
    }

    fn skip_newlines(&mut self) {
        while self.current_token.kind == TokenKind::NewLine {
            self.next_token();
        }
    }

    fn skip_newlines_in_regex_context(&mut self) {
        while self.current_token.kind == TokenKind::NewLine {
            self.next_token_in_regex_context();
        }
    }

    fn skip_terminators(&mut self) {
        while matches!(
            self.current_token.kind,
            TokenKind::NewLine | TokenKind::Semicolon
        ) {
            self.next_token();
        }
    }

    fn is_eof(&self) -> bool {
        self.current_token.kind == TokenKind::Eof
    }

    fn is_statement_terminator(&self) -> bool {
        matches!(
            self.current_token.kind,
            TokenKind::Semicolon | TokenKind::NewLine | TokenKind::RightCurlyBrace | TokenKind::Eof
        )
    }

    fn token_is_immediately_after(&self, previous: &Token<'a>) -> bool {
        self.current_token.span.start == previous.span.start + previous.literal.len()
    }

    fn parse_number_expression(&self) -> Option<Expression<'a>> {
        let literal = self.current_token.literal;
        if let Some(hex_digits) = literal
            .strip_prefix("0x")
            .or_else(|| literal.strip_prefix("0X"))
        {
            let value = u64::from_str_radix(hex_digits, 16).ok()? as f64;
            return Some(Expression::HexNumber { literal, value });
        }

        literal.parse::<f64>().ok().map(Expression::Number)
    }

    fn parse_array_index_expression(&mut self) -> Result<Expression<'a>, ParseError<'a>> {
        let mut index = self.parse_expression()?;
        while self.current_token.kind == TokenKind::Comma {
            let operator = self.current_token.clone();
            self.next_token_in_regex_context();
            let right = self.parse_expression()?;
            index = Expression::Infix {
                left: Box::new(index),
                operator,
                right: Box::new(right),
            };
        }
        Ok(index)
    }

    fn parse_error(&self, kind: ParseErrorKind) -> ParseError<'a> {
        ParseError {
            kind,
            token: self.current_token.clone(),
        }
    }

    fn expected_rule(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedRule)
    }

    fn expected_statement(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedStatement)
    }

    fn expected_identifier(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedIdentifier)
    }

    fn unsupported_statement(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::UnsupportedStatement)
    }

    fn expected_left_paren(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedLeftParen)
    }

    fn expected_left_brace(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedLeftBrace)
    }

    fn expected_right_square_bracket(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedRightSquareBracket)
    }

    fn expected_comma(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedComma)
    }

    fn expected_colon(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedColon)
    }

    fn expected_semicolon(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedSemicolon)
    }

    fn expected_while(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedWhile)
    }

    fn expected_right_brace(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedRightBrace)
    }

    fn expected_right_paren(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::ExpectedRightParen)
    }

    fn missing_printf_format_string(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::MissingPrintfFormatString)
    }

    fn invalid_numeric_literal(&self) -> ParseError<'a> {
        self.parse_error(ParseErrorKind::InvalidNumericLiteral)
    }

    fn split_print_parenthesized_list(expression: Expression<'a>) -> Option<Vec<Expression<'a>>> {
        fn flatten<'a>(expression: Expression<'a>, expressions: &mut Vec<Expression<'a>>) -> bool {
            match expression {
                Expression::Infix {
                    left,
                    operator,
                    right,
                } if operator.kind == TokenKind::Comma => {
                    flatten(*left, expressions) && flatten(*right, expressions)
                }
                other => {
                    expressions.push(other);
                    true
                }
            }
        }

        let mut expressions = Vec::new();
        if flatten(expression, &mut expressions) && expressions.len() > 1 {
            Some(expressions)
        } else {
            None
        }
    }

    fn parse_next_rule(&mut self) -> Result<Option<Rule<'a>>, ParseError<'a>> {
        match &self.current_token.kind {
            TokenKind::Begin => {
                self.next_token();
                if self.current_token.kind != TokenKind::LeftCurlyBrace {
                    return Err(self.expected_left_brace());
                }
                let action = self.parse_action()?;
                Ok(Some(Rule::Begin(action)))
            }
            TokenKind::NewLine => {
                self.next_token_in_regex_context();
                self.parse_next_rule()
            }
            TokenKind::Eof => Ok(None),
            TokenKind::LeftCurlyBrace => {
                self.parse_action().map(|action| Some(Rule::Action(action)))
            }
            TokenKind::Function => {
                self.parse_function_definition()?;
                Ok(None)
            }
            TokenKind::End => {
                self.next_token();
                if self.current_token.kind != TokenKind::LeftCurlyBrace {
                    return Err(self.expected_left_brace());
                }
                let action = self.parse_action()?;
                Ok(Some(Rule::End(action)))
            }
            TokenKind::Regex
            | TokenKind::String
            | TokenKind::Number
            | TokenKind::DollarSign
            | TokenKind::LeftParen
            | TokenKind::Identifier
            | TokenKind::Cos
            | TokenKind::Exp
            | TokenKind::Index
            | TokenKind::Int
            | TokenKind::Length
            | TokenKind::Log
            | TokenKind::Match
            | TokenKind::Rand
            | TokenKind::Sin
            | TokenKind::Sprintf
            | TokenKind::Split
            | TokenKind::Sqrt
            | TokenKind::Srand
            | TokenKind::Substr
            | TokenKind::ExclamationMark
            | TokenKind::Increment
            | TokenKind::Decrement => self.parse_pattern_rule(),
            _ => Err(self.expected_rule()),
        }
    }

    fn parse_pattern_rule(&mut self) -> Result<Option<Rule<'a>>, ParseError<'a>> {
        let mut pattern = self.parse_expression()?;
        if self.current_token.kind == TokenKind::Comma {
            let operator = self.current_token.clone();
            self.next_token_in_regex_context();
            let right = self.parse_expression()?;
            pattern = Expression::Infix {
                left: Box::new(pattern),
                operator,
                right: Box::new(right),
            };
        }
        let pattern = Some(pattern);

        if self.current_token.kind == TokenKind::LeftCurlyBrace {
            let action = self.parse_action()?;
            Ok(Some(Rule::PatternAction {
                pattern,
                action: Some(action),
            }))
        } else {
            Ok(Some(Rule::PatternAction {
                pattern,
                action: None,
            }))
        }
    }

    fn parse_action(&mut self) -> Result<Action<'a>, ParseError<'a>> {
        self.next_token(); // consume '{'

        let mut statements = Vec::new();
        while self.current_token.kind != TokenKind::RightCurlyBrace
            && self.current_token.kind != TokenKind::Eof
        {
            self.skip_terminators();

            if self.current_token.kind == TokenKind::RightCurlyBrace
                || self.current_token.kind == TokenKind::Eof
            {
                break;
            }

            statements.push(self.parse_statement()?);
        }

        if self.current_token.kind != TokenKind::RightCurlyBrace {
            return Err(self.expected_right_brace());
        }

        Ok(Action { statements })
    }

    fn parse_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        match self.current_token.kind {
            TokenKind::Print => self.parse_print_function(),
            TokenKind::Printf => self.parse_printf_function(),
            TokenKind::System => self.parse_system_function(),
            TokenKind::Split => self.parse_split_statement(),
            TokenKind::Sub => self.parse_sub_function(),
            TokenKind::Gsub => self.parse_gsub_function(),
            TokenKind::Break => Ok(self.parse_break_statement()),
            TokenKind::Continue => Ok(self.parse_continue_statement()),
            TokenKind::Delete => self.parse_delete_statement(),
            TokenKind::If => self.parse_if_statement(),
            TokenKind::Do => self.parse_do_statement(),
            TokenKind::While => self.parse_while_statement(),
            TokenKind::For => self.parse_for_statement(),
            TokenKind::Return => self.parse_return_statement(),
            TokenKind::Next => Ok(self.parse_next_statement()),
            TokenKind::Exit => self.parse_exit_statement(),
            TokenKind::Identifier => self.parse_assignment_statement(),
            TokenKind::DollarSign => self.parse_field_assignment_statement(),
            TokenKind::Increment => self.parse_pre_increment_statement(),
            TokenKind::Decrement => self.parse_pre_decrement_statement(),
            TokenKind::Number
            | TokenKind::String
            | TokenKind::Regex
            | TokenKind::LeftParen
            | TokenKind::Close
            | TokenKind::Cos
            | TokenKind::Exp
            | TokenKind::Index
            | TokenKind::Int
            | TokenKind::Length
            | TokenKind::Log
            | TokenKind::Match
            | TokenKind::Rand
            | TokenKind::Sin
            | TokenKind::Sprintf
            | TokenKind::Sqrt
            | TokenKind::Srand
            | TokenKind::Substr
            | TokenKind::ToLower
            | TokenKind::ToUpper => Ok(Statement::Expression(self.parse_expression()?)),
            _ => Err(self.expected_statement()),
        }
    }

    fn parse_function_definition(&mut self) -> Result<(), ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::Identifier {
            return Err(self.expected_identifier());
        }
        let name = self.current_token.literal;
        self.next_token();
        if self.current_token.kind != TokenKind::LeftParen {
            return Err(self.expected_left_paren());
        }
        self.next_token();

        let mut parameters = Vec::new();
        while self.current_token.kind != TokenKind::RightParen {
            if self.current_token.kind != TokenKind::Identifier {
                return Err(self.expected_identifier());
            }
            parameters.push(self.current_token.literal);
            self.next_token();
            if self.current_token.kind == TokenKind::Comma {
                self.next_token();
            } else if self.current_token.kind != TokenKind::RightParen {
                return Err(self.expected_right_paren());
            }
        }

        self.next_token();
        self.skip_newlines();
        if self.current_token.kind != TokenKind::LeftCurlyBrace {
            return Err(self.expected_left_brace());
        }

        let mut statements = Vec::new();
        self.next_token(); // consume '{'
        while self.current_token.kind != TokenKind::RightCurlyBrace
            && self.current_token.kind != TokenKind::Eof
        {
            self.skip_terminators();

            if self.current_token.kind == TokenKind::RightCurlyBrace
                || self.current_token.kind == TokenKind::Eof
            {
                break;
            }

            statements.push(self.parse_statement()?);
        }
        if self.current_token.kind != TokenKind::RightCurlyBrace {
            return Err(self.expected_right_brace());
        }
        self.function_definitions.push(FunctionDefinition {
            name,
            parameters,
            statements,
        });

        Ok(())
    }

    fn parse_simple_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        match self.current_token.kind {
            TokenKind::Identifier => self.parse_assignment_statement(),
            TokenKind::DollarSign => self.parse_field_assignment_statement(),
            TokenKind::Increment => self.parse_pre_increment_statement(),
            TokenKind::Decrement => self.parse_pre_decrement_statement(),
            TokenKind::Number
            | TokenKind::String
            | TokenKind::Regex
            | TokenKind::LeftParen
            | TokenKind::Close
            | TokenKind::Cos
            | TokenKind::Exp
            | TokenKind::Index
            | TokenKind::Int
            | TokenKind::Length
            | TokenKind::Log
            | TokenKind::Match
            | TokenKind::Rand
            | TokenKind::Sin
            | TokenKind::Sprintf
            | TokenKind::Sqrt
            | TokenKind::Srand
            | TokenKind::Substr
            | TokenKind::ToLower
            | TokenKind::ToUpper => Ok(Statement::Expression(self.parse_expression()?)),
            _ => Err(self.unsupported_statement()),
        }
    }

    fn parse_assignment_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        let identifier = self.current_token.clone();
        self.next_token();
        self.parse_assignment_statement_with_identifier(identifier)
    }

    fn parse_assignment_statement_with_identifier(
        &mut self,
        identifier: Token<'a>,
    ) -> Result<Statement<'a>, ParseError<'a>> {
        if self.current_token.kind == TokenKind::LeftParen
            && self.token_is_immediately_after(&identifier)
        {
            let args = self.parse_call_arguments()?;
            return Ok(Statement::Expression(Expression::FunctionCall {
                name: identifier.literal,
                args,
            }));
        }
        if self.current_token.kind == TokenKind::LeftSquareBracket {
            self.next_token_in_regex_context();
            let index = self.parse_array_index_expression()?;
            if self.current_token.kind != TokenKind::RightSquareBracket {
                return Err(self.expected_right_square_bracket());
            }
            self.next_token();
            return match self.current_token.kind {
                TokenKind::Assign => {
                    self.next_token_in_regex_context();
                    let value = self.parse_expression()?;
                    Ok(Statement::ArrayAssignment {
                        identifier: identifier.literal,
                        index,
                        value,
                    })
                }
                TokenKind::AddAssign => {
                    self.next_token_in_regex_context();
                    let value = self.parse_expression()?;
                    Ok(Statement::ArrayAddAssignment {
                        identifier: identifier.literal,
                        index,
                        value,
                    })
                }
                TokenKind::Increment => {
                    self.next_token();
                    Ok(Statement::ArrayPostIncrement {
                        identifier: identifier.literal,
                        index,
                    })
                }
                TokenKind::Decrement => {
                    self.next_token();
                    Ok(Statement::ArrayPostDecrement {
                        identifier: identifier.literal,
                        index,
                    })
                }
                _ => Err(self.unsupported_statement()),
            };
        }
        match self.current_token.kind {
            TokenKind::Assign => {
                self.next_token_in_regex_context();
                if self.current_token.kind == TokenKind::Split {
                    return self.parse_split_assignment_statement(identifier.literal);
                }
                let value = self.parse_expression()?;
                Ok(Statement::Assignment {
                    identifier: identifier.literal,
                    value,
                })
            }
            TokenKind::Increment => {
                self.next_token();
                Ok(Statement::PostIncrement {
                    identifier: identifier.literal,
                })
            }
            TokenKind::Decrement => {
                self.next_token();
                Ok(Statement::PostDecrement {
                    identifier: identifier.literal,
                })
            }
            TokenKind::AddAssign => {
                self.next_token_in_regex_context();
                let value = self.parse_expression()?;
                Ok(Statement::AddAssignment {
                    identifier: identifier.literal,
                    value,
                })
            }
            TokenKind::SubtractAssign
            | TokenKind::MultiplyAssign
            | TokenKind::DivideAssign
            | TokenKind::ModuloAssign
            | TokenKind::PowerAssign => {
                let assign_token = self.current_token.clone();
                self.next_token_in_regex_context();
                let right_value = self.parse_expression()?;
                Ok(Statement::Assignment {
                    identifier: identifier.literal,
                    value: Expression::Infix {
                        left: Box::new(Expression::Identifier(identifier.literal)),
                        operator: compound_assign_operator(&assign_token),
                        right: Box::new(right_value),
                    },
                })
            }
            _ => Err(self.unsupported_statement()),
        }
    }

    fn parse_delete_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::Identifier {
            return Err(self.expected_identifier());
        }
        let identifier = self.current_token.literal;
        self.next_token();
        if self.current_token.kind != TokenKind::LeftSquareBracket {
            return Ok(Statement::Delete {
                identifier,
                index: None,
            });
        }

        self.next_token_in_regex_context();
        let index = self.parse_array_index_expression()?;
        if self.current_token.kind != TokenKind::RightSquareBracket {
            return Err(self.expected_right_square_bracket());
        }
        self.next_token();
        Ok(Statement::Delete {
            identifier,
            index: Some(index),
        })
    }

    fn parse_break_statement(&mut self) -> Statement<'a> {
        self.next_token();
        Statement::Break
    }

    fn parse_continue_statement(&mut self) -> Statement<'a> {
        self.next_token();
        Statement::Continue
    }

    fn parse_pre_increment_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::Identifier {
            return Err(self.expected_identifier());
        }
        let identifier = self.current_token.literal;
        self.next_token();
        Ok(Statement::PreIncrement { identifier })
    }

    fn parse_pre_decrement_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::Identifier {
            return Err(self.expected_identifier());
        }
        let identifier = self.current_token.literal;
        self.next_token();
        Ok(Statement::PreDecrement { identifier })
    }

    fn parse_split_assignment_statement(
        &mut self,
        identifier: &'a str,
    ) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::LeftParen {
            return Err(self.expected_left_paren());
        }
        self.next_token_in_regex_context();
        let string = self.parse_expression()?;
        if self.current_token.kind != TokenKind::Comma {
            return Err(self.expected_comma());
        }
        self.next_token();
        if self.current_token.kind != TokenKind::Identifier {
            return Err(self.expected_identifier());
        }
        let array = self.current_token.literal;
        self.next_token();
        let separator = if self.current_token.kind == TokenKind::Comma {
            self.next_token_in_regex_context();
            Some(self.parse_expression()?)
        } else {
            None
        };
        if self.current_token.kind != TokenKind::RightParen {
            return Err(self.expected_right_paren());
        }
        self.next_token();
        Ok(Statement::SplitAssignment {
            identifier,
            string,
            array,
            separator,
        })
    }

    fn parse_split_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::LeftParen {
            return Err(self.expected_left_paren());
        }
        self.next_token_in_regex_context();
        let string = self.parse_expression()?;
        if self.current_token.kind != TokenKind::Comma {
            return Err(self.expected_comma());
        }
        self.next_token();
        if self.current_token.kind != TokenKind::Identifier {
            return Err(self.expected_identifier());
        }
        let array = self.current_token.literal;
        self.next_token();
        let separator = if self.current_token.kind == TokenKind::Comma {
            self.next_token_in_regex_context();
            Some(self.parse_expression()?)
        } else {
            None
        };
        if self.current_token.kind != TokenKind::RightParen {
            return Err(self.expected_right_paren());
        }
        self.next_token();
        Ok(Statement::Split {
            string,
            array,
            separator,
        })
    }

    fn parse_field_assignment_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        let field = self.parse_primary_expression()?;
        let assign_token = self.current_token.clone();
        self.next_token_in_regex_context();
        let right_value = self.parse_expression()?;

        let value = if assign_token.kind == TokenKind::Assign {
            right_value
        } else {
            let operator = compound_assign_operator(&assign_token);
            Expression::Infix {
                left: Box::new(Expression::Field(Box::new(field.clone()))),
                operator,
                right: Box::new(right_value),
            }
        };
        Ok(Statement::FieldAssignment { field, value })
    }

    fn parse_if_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::LeftParen {
            return Err(self.expected_left_paren());
        }
        self.next_token_in_regex_context();
        let condition = self.parse_condition_in_parens()?;
        if self.current_token.kind != TokenKind::RightParen {
            return Err(self.expected_right_paren());
        }
        self.next_token();
        let then_statements = self.parse_control_statement_body()?;

        self.skip_terminators();

        if self.current_token.kind == TokenKind::Else {
            self.next_token();
            let else_statements = self.parse_control_statement_body()?;
            return Ok(Statement::IfElse {
                condition,
                then_statements,
                else_statements,
            });
        }

        Ok(Statement::If {
            condition,
            then_statements,
        })
    }

    fn parse_exit_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        let status = if self.is_statement_terminator() {
            None
        } else {
            Some(self.parse_expression()?)
        };
        Ok(Statement::Exit(status))
    }

    fn parse_return_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        let value = if self.is_statement_terminator() {
            None
        } else {
            Some(self.parse_expression()?)
        };
        Ok(Statement::Return(value))
    }

    fn parse_next_statement(&mut self) -> Statement<'a> {
        self.next_token();
        Statement::Next
    }

    fn parse_statement_block(&mut self) -> Result<Vec<Statement<'a>>, ParseError<'a>> {
        self.next_token(); // consume '{'
        let mut statements = Vec::new();
        while self.current_token.kind != TokenKind::RightCurlyBrace
            && self.current_token.kind != TokenKind::Eof
        {
            self.skip_terminators();

            if self.current_token.kind == TokenKind::RightCurlyBrace
                || self.current_token.kind == TokenKind::Eof
            {
                break;
            }
            statements.push(self.parse_statement()?);
        }
        if self.current_token.kind != TokenKind::RightCurlyBrace {
            return Err(self.expected_right_brace());
        }
        self.next_token();
        Ok(statements)
    }

    fn parse_control_statement_body(&mut self) -> Result<Vec<Statement<'a>>, ParseError<'a>> {
        self.skip_newlines();

        if self.current_token.kind == TokenKind::LeftCurlyBrace {
            return self.parse_statement_block();
        }

        if self.current_token.kind == TokenKind::Semicolon {
            self.next_token();
            return Ok(vec![Statement::Empty]);
        }

        Ok(vec![self.parse_statement()?])
    }

    fn parse_while_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::LeftParen {
            return Err(self.expected_left_paren());
        }
        self.next_token_in_regex_context();
        let condition = self.parse_condition_in_parens()?;
        if self.current_token.kind != TokenKind::RightParen {
            return Err(self.expected_right_paren());
        }
        self.next_token();
        let statements = self.parse_control_statement_body()?;
        Ok(Statement::While {
            condition,
            statements,
        })
    }

    fn parse_do_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        let statements = self.parse_control_statement_body()?;

        self.skip_terminators();

        if self.current_token.kind != TokenKind::While {
            return Err(self.expected_while());
        }
        self.next_token();
        if self.current_token.kind != TokenKind::LeftParen {
            return Err(self.expected_left_paren());
        }
        self.next_token_in_regex_context();
        let condition = self.parse_condition_in_parens()?;
        if self.current_token.kind != TokenKind::RightParen {
            return Err(self.expected_right_paren());
        }
        self.next_token();
        Ok(Statement::DoWhile {
            condition,
            statements,
        })
    }

    fn parse_for_statement(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::LeftParen {
            return Err(self.expected_left_paren());
        }
        self.next_token();
        self.skip_newlines();

        let init = if self.current_token.kind == TokenKind::Semicolon {
            Statement::Empty
        } else if self.current_token.kind == TokenKind::Identifier {
            let variable = self.current_token.clone();
            self.next_token();
            if self.current_token.kind == TokenKind::In {
                self.next_token();
                if self.current_token.kind != TokenKind::Identifier {
                    return Err(self.expected_identifier());
                }
                let array = self.current_token.literal;
                self.next_token();
                if self.current_token.kind != TokenKind::RightParen {
                    return Err(self.expected_right_paren());
                }
                self.next_token();
                let statements = self.parse_control_statement_body()?;
                return Ok(Statement::ForIn {
                    variable: variable.literal,
                    array,
                    statements,
                });
            }
            self.parse_assignment_statement_with_identifier(variable)?
        } else {
            self.parse_simple_statement()?
        };
        self.skip_newlines();
        if self.current_token.kind != TokenKind::Semicolon {
            return Err(self.expected_semicolon());
        }
        self.next_token_in_regex_context();
        self.skip_newlines_in_regex_context();

        let condition = if self.current_token.kind == TokenKind::Semicolon {
            Expression::Number(1.0)
        } else {
            self.parse_expression()?
        };
        self.skip_newlines();
        if self.current_token.kind != TokenKind::Semicolon {
            return Err(self.expected_semicolon());
        }
        self.next_token_in_regex_context();
        self.skip_newlines_in_regex_context();

        let update = if self.current_token.kind == TokenKind::RightParen {
            Statement::Empty
        } else {
            self.parse_simple_statement()?
        };
        self.skip_newlines();
        if self.current_token.kind != TokenKind::RightParen {
            return Err(self.expected_right_paren());
        }
        self.next_token();
        let statements = self.parse_control_statement_body()?;

        Ok(Statement::For {
            init: Box::new(init),
            condition,
            update: Box::new(update),
            statements,
        })
    }

    fn parse_print_function(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        let mut expressions = Vec::new();
        let mut expect_more = false;
        self.next_token();

        loop {
            if self.current_token.kind == TokenKind::RightCurlyBrace
                || self.current_token.kind == TokenKind::RightParen
                || self.current_token.kind == TokenKind::Eof
                || self.current_token.kind == TokenKind::GreaterThan
                || self.current_token.kind == TokenKind::Append
                || self.current_token.kind == TokenKind::Pipe
            {
                break;
            }

            if self.current_token.kind == TokenKind::NewLine
                || self.current_token.kind == TokenKind::Semicolon
            {
                if expect_more {
                    self.next_token();
                    continue;
                }
                break;
            }

            if self.current_token.kind == TokenKind::Comma {
                self.next_token();
                expect_more = true;
                continue;
            }

            let started_with_left_paren = self.current_token.kind == TokenKind::LeftParen;
            let expression = self.parse_expression()?;
            if started_with_left_paren {
                if let Some(grouped_expressions) =
                    Self::split_print_parenthesized_list(expression.clone())
                {
                    expressions.extend(grouped_expressions);
                } else {
                    expressions.push(expression);
                }
            } else {
                expressions.push(expression);
            }
            expect_more = false;
        }
        if self.current_token.kind == TokenKind::GreaterThan
            || self.current_token.kind == TokenKind::Append
        {
            let append = self.current_token.kind == TokenKind::Append;
            self.next_token();
            let target = self.parse_expression()?;
            return Ok(Statement::PrintRedirect {
                expressions,
                target,
                append,
            });
        }
        if self.current_token.kind == TokenKind::Pipe {
            self.next_token();
            let target = self.parse_expression()?;
            return Ok(Statement::PrintPipe {
                expressions,
                target,
            });
        }

        Ok(Statement::Print(expressions))
    }

    fn parse_printf_function(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        let expressions = if self.current_token.kind == TokenKind::LeftParen {
            self.next_token_in_regex_context();
            let mut expressions = Vec::new();
            while self.current_token.kind != TokenKind::RightParen
                && self.current_token.kind != TokenKind::Eof
            {
                if self.current_token.kind == TokenKind::Comma {
                    self.next_token();
                    continue;
                }
                expressions.push(self.parse_expression()?);
            }
            if self.current_token.kind == TokenKind::RightParen {
                self.next_token();
            }
            expressions
        } else {
            self.parse_expression_list_until_action_end_from_current()?
        };

        if expressions.is_empty() {
            return Err(self.missing_printf_format_string());
        }

        Ok(Statement::Printf(expressions))
    }

    fn parse_gsub_function(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::LeftParen {
            return Err(self.expected_left_paren());
        }

        self.next_token_in_regex_context();
        let pattern = self.parse_expression()?;

        if self.current_token.kind != TokenKind::Comma {
            return Err(self.expected_comma());
        }
        self.next_token();
        let replacement = self.parse_expression()?;

        let target = if self.current_token.kind == TokenKind::Comma {
            self.next_token();
            Some(self.parse_expression()?)
        } else {
            None
        };

        if self.current_token.kind != TokenKind::RightParen {
            return Err(self.expected_right_paren());
        }
        self.next_token();

        Ok(Statement::Gsub {
            pattern,
            replacement,
            target,
        })
    }

    fn parse_sub_function(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::LeftParen {
            return Err(self.expected_left_paren());
        }

        self.next_token_in_regex_context();
        let pattern = self.parse_expression()?;

        if self.current_token.kind != TokenKind::Comma {
            return Err(self.expected_comma());
        }
        self.next_token();
        let replacement = self.parse_expression()?;

        let target = if self.current_token.kind == TokenKind::Comma {
            self.next_token();
            Some(self.parse_expression()?)
        } else {
            None
        };

        if self.current_token.kind != TokenKind::RightParen {
            return Err(self.expected_right_paren());
        }
        self.next_token();

        Ok(Statement::Sub {
            pattern,
            replacement,
            target,
        })
    }

    fn parse_system_function(&mut self) -> Result<Statement<'a>, ParseError<'a>> {
        self.next_token();
        if self.current_token.kind != TokenKind::LeftParen {
            return Err(self.expected_left_paren());
        }
        self.next_token();
        let command = self.parse_expression()?;
        if self.current_token.kind != TokenKind::RightParen {
            return Err(self.expected_right_paren());
        }
        self.next_token();
        Ok(Statement::System(command))
    }

    fn parse_expression_list_until_action_end_from_current(
        &mut self,
    ) -> Result<Vec<Expression<'a>>, ParseError<'a>> {
        let mut expressions = Vec::new();
        let mut expect_more = false;

        loop {
            if self.current_token.kind == TokenKind::RightCurlyBrace
                || self.current_token.kind == TokenKind::RightParen
                || self.current_token.kind == TokenKind::Eof
            {
                break;
            }

            if self.current_token.kind == TokenKind::NewLine
                || self.current_token.kind == TokenKind::Semicolon
            {
                if expect_more {
                    self.next_token();
                    continue;
                }
                break;
            }

            if self.current_token.kind == TokenKind::Comma {
                self.next_token();
                expect_more = true;
                continue;
            }

            let started_with_left_paren = self.current_token.kind == TokenKind::LeftParen;
            let expression = self.parse_expression()?;
            expressions.push(expression);
            if started_with_left_paren && self.current_token.kind == TokenKind::Comma {
                while self.current_token.kind == TokenKind::Comma {
                    self.next_token();
                    expressions.push(self.parse_expression()?);
                }
                if self.current_token.kind != TokenKind::RightParen {
                    return Err(self.expected_right_paren());
                }
                self.next_token();
            }
            expect_more = false;
        }

        Ok(expressions)
    }

    fn parse_expression(&mut self) -> Result<Expression<'a>, ParseError<'a>> {
        self.parse_expression_with_min_precedence(0)
    }

    fn parse_expression_with_min_precedence(
        &mut self,
        min_precedence: u8,
    ) -> Result<Expression<'a>, ParseError<'a>> {
        let left = self.parse_primary_expression()?;
        self.parse_expression_suffix(left, min_precedence)
    }

    fn parse_expression_suffix(
        &mut self,
        mut left: Expression<'a>,
        min_precedence: u8,
    ) -> Result<Expression<'a>, ParseError<'a>> {
        const CONCAT_LEFT_PRECEDENCE: u8 = 6;
        const CONCAT_RIGHT_PRECEDENCE: u8 = 7;

        loop {
            if self.current_token.kind == TokenKind::QuestionMark {
                if min_precedence > 0 {
                    break;
                }
                self.next_token_in_regex_context();
                let then_expr = self.parse_expression_with_min_precedence(0)?;
                if self.current_token.kind != TokenKind::Colon {
                    return Err(self.expected_colon());
                }
                self.next_token_in_regex_context();
                let else_expr = self.parse_expression_with_min_precedence(0)?;
                left = Expression::Ternary {
                    condition: Box::new(left),
                    then_expr: Box::new(then_expr),
                    else_expr: Box::new(else_expr),
                };
                continue;
            }

            if infix_operator_precedence(&self.current_token.kind).is_none()
                && is_expression_start(&self.current_token.kind)
            {
                if CONCAT_LEFT_PRECEDENCE < min_precedence {
                    break;
                }

                let right = self.parse_expression_with_min_precedence(CONCAT_RIGHT_PRECEDENCE)?;
                left = Expression::Concatenation {
                    left: Box::new(left),
                    right: Box::new(right),
                };
                continue;
            }

            let (left_precedence, right_precedence) =
                match infix_operator_precedence(&self.current_token.kind) {
                    Some(value) => value,
                    None => break,
                };

            if left_precedence < min_precedence {
                break;
            }

            let operator = self.current_token.clone();
            if matches!(
                operator.kind,
                TokenKind::Tilde | TokenKind::NoMatch | TokenKind::And | TokenKind::Or
            ) {
                self.next_token_in_regex_context();
            } else {
                self.next_token();
            }
            let right = self.parse_expression_with_min_precedence(right_precedence)?;

            left = Expression::Infix {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            };
        }

        Ok(left)
    }

    fn parse_condition_in_parens(&mut self) -> Result<Expression<'a>, ParseError<'a>> {
        let mut condition = self.parse_expression()?;
        if self.current_token.kind == TokenKind::Comma {
            while self.current_token.kind == TokenKind::Comma {
                let operator = self.current_token.clone();
                self.next_token_in_regex_context();
                let right = self.parse_expression()?;
                condition = Expression::Infix {
                    left: Box::new(condition),
                    operator,
                    right: Box::new(right),
                };
            }
            if self.current_token.kind != TokenKind::RightParen {
                return Err(self.expected_right_paren());
            }
            self.next_token();
            condition = self.parse_expression_suffix(condition, 0)?;
        }
        Ok(condition)
    }

    fn parse_primary_expression(&mut self) -> Result<Expression<'a>, ParseError<'a>> {
        if self.current_token.kind == TokenKind::Minus {
            let operator = self.current_token.clone();
            self.next_token();
            let right = self.parse_primary_expression()?;
            return Ok(Expression::Infix {
                left: Box::new(Expression::Number(0.0)),
                operator,
                right: Box::new(right),
            });
        }
        if self.current_token.kind == TokenKind::Plus {
            self.next_token();
            return self.parse_primary_expression();
        }
        if self.current_token.kind == TokenKind::ExclamationMark {
            self.next_token_in_regex_context();
            let expression = self.parse_primary_expression()?;
            return Ok(Expression::Not(Box::new(expression)));
        }
        if self.current_token.kind == TokenKind::Increment {
            self.next_token();
            let expression = self.parse_primary_expression()?;
            return Ok(Expression::PreIncrement(Box::new(expression)));
        }
        if self.current_token.kind == TokenKind::Decrement {
            self.next_token();
            let expression = self.parse_primary_expression()?;
            return Ok(Expression::PreDecrement(Box::new(expression)));
        }

        let mut expression = self.parse_primary_atom()?;
        match self.current_token.kind {
            TokenKind::Increment => {
                self.next_token();
                expression = Expression::PostIncrement(Box::new(expression));
            }
            TokenKind::Decrement => {
                self.next_token();
                expression = Expression::PostDecrement(Box::new(expression));
            }
            _ => {}
        }
        Ok(expression)
    }

    fn parse_primary_atom(&mut self) -> Result<Expression<'a>, ParseError<'a>> {
        match self.current_token.kind {
            TokenKind::String => {
                let expression = Expression::String(self.current_token.literal);
                self.next_token();
                Ok(expression)
            }
            TokenKind::Regex => {
                let expression = Expression::Regex(self.current_token.literal);
                self.next_token();
                Ok(expression)
            }
            TokenKind::Number => {
                let expression = self
                    .parse_number_expression()
                    .ok_or_else(|| self.invalid_numeric_literal())?;
                self.next_token();
                Ok(expression)
            }
            TokenKind::DollarSign => {
                self.next_token();
                let expression = self.parse_primary_atom()?;
                Ok(Expression::Field(Box::new(expression)))
            }
            TokenKind::LeftParen => {
                self.next_token_in_regex_context();
                let mut expression = self.parse_expression()?;
                while self.current_token.kind == TokenKind::Comma {
                    let operator = self.current_token.clone();
                    self.next_token_in_regex_context();
                    let right = self.parse_expression()?;
                    expression = Expression::Infix {
                        left: Box::new(expression),
                        operator,
                        right: Box::new(right),
                    };
                }
                if self.current_token.kind != TokenKind::RightParen {
                    return Err(self.expected_right_paren());
                }
                self.next_token();
                Ok(expression)
            }
            TokenKind::Identifier => {
                let identifier = self.current_token.clone();
                self.next_token();
                if self.current_token.kind == TokenKind::LeftParen
                    && self.token_is_immediately_after(&identifier)
                {
                    let args = self.parse_call_arguments()?;
                    return Ok(Expression::FunctionCall {
                        name: identifier.literal,
                        args,
                    });
                }
                if self.current_token.kind == TokenKind::LeftSquareBracket {
                    self.next_token_in_regex_context();
                    let index = self.parse_array_index_expression()?;
                    if self.current_token.kind != TokenKind::RightSquareBracket {
                        return Err(self.expected_right_square_bracket());
                    }
                    self.next_token();
                    Ok(Expression::ArrayAccess {
                        identifier: identifier.literal,
                        index: Box::new(index),
                    })
                } else {
                    Ok(Expression::Identifier(identifier.literal))
                }
            }
            TokenKind::Length => {
                self.next_token();
                if self.current_token.kind == TokenKind::LeftParen {
                    self.next_token();
                    if self.current_token.kind == TokenKind::RightParen {
                        self.next_token();
                        Ok(Expression::Length(None))
                    } else {
                        let expression = self.parse_expression()?;
                        if self.current_token.kind != TokenKind::RightParen {
                            return Err(self.expected_right_paren());
                        }
                        self.next_token();
                        Ok(Expression::Length(Some(Box::new(expression))))
                    }
                } else {
                    Ok(Expression::Length(None))
                }
            }
            TokenKind::Substr => {
                self.next_token();
                if self.current_token.kind != TokenKind::LeftParen {
                    return Err(self.expected_left_paren());
                }
                self.next_token();
                let string = self.parse_expression()?;
                if self.current_token.kind != TokenKind::Comma {
                    return Err(self.expected_comma());
                }
                self.next_token();
                let start = self.parse_expression()?;
                let mut length = None;
                if self.current_token.kind == TokenKind::Comma {
                    self.next_token();
                    length = Some(Box::new(self.parse_expression()?));
                }
                if self.current_token.kind != TokenKind::RightParen {
                    return Err(self.expected_right_paren());
                }
                self.next_token();
                Ok(Expression::Substr {
                    string: Box::new(string),
                    start: Box::new(start),
                    length,
                })
            }
            TokenKind::Rand => {
                self.next_token();
                if self.current_token.kind == TokenKind::LeftParen {
                    self.next_token();
                    if self.current_token.kind != TokenKind::RightParen {
                        return Err(self.expected_right_paren());
                    }
                    self.next_token();
                }
                Ok(Expression::Rand)
            }
            TokenKind::Close
            | TokenKind::Cos
            | TokenKind::Exp
            | TokenKind::Index
            | TokenKind::Int
            | TokenKind::Log
            | TokenKind::Match
            | TokenKind::Sin
            | TokenKind::Sprintf
            | TokenKind::Split
            | TokenKind::Sqrt
            | TokenKind::Srand => {
                let name = self.current_token.literal;
                self.next_token();
                if self.current_token.kind == TokenKind::LeftParen {
                    let args = self.parse_call_arguments()?;
                    return Ok(Expression::FunctionCall { name, args });
                }
                Err(self.expected_left_paren())
            }
            _ => Err(self.expected_statement()),
        }
    }

    pub fn try_parse_program(&mut self) -> Result<Program<'_>, ParseError<'a>> {
        let mut program = Program::new();

        while !self.is_eof() {
            match self.parse_next_rule()? {
                Some(Rule::Begin(action)) => program.add_begin_block(action),
                Some(Rule::End(action)) => program.add_end_block(action),
                Some(rule) => program.add_rule(rule),
                None => {}
            }
            self.next_token_in_regex_context();
        }

        for definition in self.function_definitions.drain(..) {
            program.add_function_definition(definition);
        }

        Ok(program)
    }

    pub fn parse_program(&mut self) -> Program<'_> {
        self.try_parse_program()
            .unwrap_or_else(|err| panic!("{err}"))
    }

    fn parse_call_arguments(&mut self) -> Result<Vec<Expression<'a>>, ParseError<'a>> {
        if self.current_token.kind != TokenKind::LeftParen {
            return Ok(vec![]);
        }
        self.next_token_in_regex_context();
        let mut args = Vec::new();
        while self.current_token.kind != TokenKind::RightParen
            && self.current_token.kind != TokenKind::Eof
        {
            if self.current_token.kind == TokenKind::Comma {
                self.next_token();
                continue;
            }
            args.push(self.parse_expression()?);
        }
        if self.current_token.kind == TokenKind::RightParen {
            self.next_token();
        }
        Ok(args)
    }
}

fn infix_operator_precedence(kind: &TokenKind) -> Option<(u8, u8)> {
    match kind {
        TokenKind::Assign
        | TokenKind::AddAssign
        | TokenKind::SubtractAssign
        | TokenKind::MultiplyAssign
        | TokenKind::DivideAssign
        | TokenKind::ModuloAssign
        | TokenKind::PowerAssign => Some((0, 0)),
        TokenKind::Or => Some((1, 2)),
        TokenKind::And => Some((3, 4)),
        TokenKind::Equal
        | TokenKind::NotEqual
        | TokenKind::GreaterThan
        | TokenKind::GreaterThanOrEqual
        | TokenKind::In
        | TokenKind::LessThan
        | TokenKind::LessThanOrEqual
        | TokenKind::Tilde
        | TokenKind::NoMatch => Some((5, 6)),
        TokenKind::Plus | TokenKind::Minus => Some((7, 8)),
        TokenKind::Asterisk | TokenKind::Division | TokenKind::Percent => Some((9, 10)),
        TokenKind::Caret => Some((13, 12)),
        _ => None,
    }
}

fn is_expression_start(kind: &TokenKind) -> bool {
    matches!(
        kind,
        TokenKind::String
            | TokenKind::Regex
            | TokenKind::Number
            | TokenKind::DollarSign
            | TokenKind::LeftParen
            | TokenKind::Identifier
            | TokenKind::Cos
            | TokenKind::Exp
            | TokenKind::Index
            | TokenKind::Int
            | TokenKind::Length
            | TokenKind::Log
            | TokenKind::Match
            | TokenKind::Rand
            | TokenKind::Sin
            | TokenKind::Sprintf
            | TokenKind::Split
            | TokenKind::Sqrt
            | TokenKind::Srand
            | TokenKind::Substr
            | TokenKind::Increment
            | TokenKind::Decrement
    )
}

fn compound_assign_operator(token: &Token<'_>) -> Token<'static> {
    let (kind, literal) = match token.kind {
        TokenKind::AddAssign => (TokenKind::Plus, "+"),
        TokenKind::SubtractAssign => (TokenKind::Minus, "-"),
        TokenKind::MultiplyAssign => (TokenKind::Asterisk, "*"),
        TokenKind::DivideAssign => (TokenKind::Division, "/"),
        TokenKind::ModuloAssign => (TokenKind::Percent, "%"),
        TokenKind::PowerAssign => (TokenKind::Caret, "^"),
        _ => unreachable!(
            "compound_assign_operator called with non-compound token: {:?}",
            token.kind
        ),
    };

    Token::new(kind, literal, token.span.start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn create_parser() {
        let mut parser = Parser::new(Lexer::new("42 == 42"));

        assert_eq!(parser.current_token.literal, "42");
        parser.next_token();
        assert_eq!(parser.current_token.literal, "==");
    }

    #[test]
    fn parse_empty_program() {
        let mut parser = Parser::new(Lexer::new(""));

        let program = parser.parse_program();

        assert_eq!(program.len(), 0);
    }

    #[test]
    fn parse_statement_with_unhandled_token_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { else }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for stray else");

        assert_eq!(err.kind, ParseErrorKind::ExpectedStatement);
        assert_eq!(err.token.kind, TokenKind::Else);
    }

    #[test]
    fn parse_begin_without_left_brace_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN print }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing left brace");

        assert_eq!(err.kind, ParseErrorKind::ExpectedLeftBrace);
        assert_eq!(err.token.kind, TokenKind::Print);
    }

    #[test]
    fn parse_delete_without_identifier_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("{ delete 1 }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for delete without identifier");

        assert_eq!(err.kind, ParseErrorKind::ExpectedIdentifier);
        assert_eq!(err.token.kind, TokenKind::Number);
    }

    #[test]
    fn parse_if_without_right_paren_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("{ if (x print }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing right paren");

        assert_eq!(err.kind, ParseErrorKind::ExpectedRightParen);
        assert_eq!(err.token.kind, TokenKind::Print);
    }

    #[test]
    fn parse_if_without_left_paren_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("{ if x) print }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing left paren after if");

        assert_eq!(err.kind, ParseErrorKind::ExpectedLeftParen);
        assert_eq!(err.token.kind, TokenKind::Identifier);
    }

    #[test]
    fn parse_grouped_expression_without_right_paren_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { print (1 + 2 }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing right paren in grouped expression");

        assert_eq!(err.kind, ParseErrorKind::ExpectedRightParen);
        assert_eq!(err.token.kind, TokenKind::RightCurlyBrace);
    }

    #[test]
    fn parse_array_assignment_without_right_square_bracket_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { a[1 = 2 }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing right square bracket");

        assert_eq!(err.kind, ParseErrorKind::ExpectedRightSquareBracket);
        assert_eq!(err.token.kind, TokenKind::RightCurlyBrace);
    }

    #[test]
    fn parse_split_without_comma_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { split($0 arr) }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing comma in split");

        assert_eq!(err.kind, ParseErrorKind::ExpectedComma);
        assert_eq!(err.token.kind, TokenKind::RightParen);
    }

    #[test]
    fn parse_identifier_expression_statement_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { x + 1 }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for unsupported identifier expression statement");

        assert_eq!(err.kind, ParseErrorKind::UnsupportedStatement);
        assert_eq!(err.token.kind, TokenKind::Plus);
    }

    #[test]
    fn parse_array_multiply_assignment_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { a[1] *= 2 }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for unsupported array compound assignment");

        assert_eq!(err.kind, ParseErrorKind::UnsupportedStatement);
        assert_eq!(err.token.kind, TokenKind::MultiplyAssign);
    }

    #[test]
    fn parse_sub_with_target() {
        let mut parser = Parser::new(Lexer::new(r#"BEGIN { sub(/a/, "b", t) }"#));

        let program = parser.parse_program();

        assert_eq!(r#"BEGIN { sub(/a/, "b", t) }"#, program.to_string());
    }

    #[test]
    fn parse_ternary_without_colon_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { print 1 ? 2 }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing colon in ternary");

        assert_eq!(err.kind, ParseErrorKind::ExpectedColon);
        assert_eq!(err.token.kind, TokenKind::RightCurlyBrace);
    }

    #[test]
    fn parse_print_with_extra_right_paren_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { print 1) }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for stray right paren after print expression");

        assert_eq!(err.kind, ParseErrorKind::ExpectedStatement);
        assert_eq!(err.token.kind, TokenKind::RightParen);
    }

    #[test]
    fn parse_for_without_first_semicolon_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { for (i = 0 i < 3; i++) print i }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing first semicolon in for");

        assert_eq!(err.kind, ParseErrorKind::ExpectedSemicolon);
        assert_eq!(err.token.kind, TokenKind::RightParen);
    }

    #[test]
    fn parse_do_without_while_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { do print 1 }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing while after do body");

        assert_eq!(err.kind, ParseErrorKind::ExpectedWhile);
        assert_eq!(err.token.kind, TokenKind::RightCurlyBrace);
    }

    #[test]
    #[should_panic(expected = "compound_assign_operator called with non-compound token")]
    fn compound_assign_operator_panics_for_non_compound_token() {
        let token = Token::new(TokenKind::Assign, "=", 0);
        let _ = compound_assign_operator(&token);
    }

    #[test]
    fn parse_printf_expression_list_with_extra_right_paren_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new(r#"BEGIN { printf "%s", 1) }"#));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for stray right paren after printf arguments");

        assert_eq!(err.kind, ParseErrorKind::ExpectedStatement);
        assert_eq!(err.token.kind, TokenKind::RightParen);
    }

    #[test]
    fn parse_action_without_right_brace_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("BEGIN { print 1"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing right brace in action");

        assert_eq!(err.kind, ParseErrorKind::ExpectedRightBrace);
        assert_eq!(err.token.kind, TokenKind::Eof);
    }

    #[test]
    fn parse_nested_block_without_right_brace_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("{ if (1) { print 1 }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for missing right brace in nested block");

        assert_eq!(err.kind, ParseErrorKind::ExpectedRightBrace);
        assert_eq!(err.token.kind, TokenKind::Eof);
    }

    #[test]
    fn parse_action_without_pattern() {
        let mut parser = Parser::new(Lexer::new("{ print }"));

        let program = parser.parse_program();

        assert_eq!(program.len(), 1);
        assert_eq!("{ print }", program.to_string());
    }

    #[test]
    fn parse_action_with_leading_newlines() {
        let mut parser = Parser::new(Lexer::new("\n\n{ print }"));

        let program = parser.parse_program();

        assert_eq!(program.len(), 1);
        assert_eq!("{ print }", program.to_string());
    }

    #[test]
    fn parse_begin_block() {
        let mut parser = Parser::new(Lexer::new("BEGIN { print }"));

        let program = parser.parse_program();

        assert_eq!(program.len(), 1);
        assert_eq!("BEGIN { print }", program.to_string());
    }

    #[test]
    fn parse_end_block() {
        let mut parser = Parser::new(Lexer::new("END { print 42 }"));

        let program = parser.parse_program();

        assert_eq!(program.len(), 1);
        assert_eq!("END { print 42 }", program.to_string());
    }

    #[test]
    fn parse_regex_pattern_action() {
        let mut parser = Parser::new(Lexer::new("/foo/ { print }"));

        let program = parser.parse_program();

        assert_eq!(program.len(), 1);
        assert_eq!("/foo/ { print }", program.to_string());
    }

    #[test]
    fn parse_print_infix_expression() {
        let mut parser = Parser::new(Lexer::new("BEGIN { print 1 + 2 }"));

        let program = parser.parse_program();
        let mut begin_blocks = program.begin_blocks_iter();
        let Action { statements } = begin_blocks.next().expect("expected begin block");

        let exprs = match &statements[0] {
            Statement::Print(expressions) => expressions,
            _ => panic!("expected print statement"),
        };

        match &exprs[0] {
            Expression::Infix {
                left,
                operator,
                right,
            } => {
                assert!(matches!(**left, Expression::Number(1.0)));
                assert_eq!(operator.kind, TokenKind::Plus);
                assert!(matches!(**right, Expression::Number(2.0)));
            }
            _ => panic!("expected infix expression"),
        }
    }

    #[test]
    fn parse_print_parenthesized_expression() {
        let mut parser = Parser::new(Lexer::new("BEGIN { print (1 + 2) * 3 }"));

        let program = parser.parse_program();
        let mut begin_blocks = program.begin_blocks_iter();
        let Action { statements } = begin_blocks.next().expect("expected begin block");

        let exprs = match &statements[0] {
            Statement::Print(expressions) => expressions,
            _ => panic!("expected print statement"),
        };

        match &exprs[0] {
            Expression::Infix {
                left,
                operator,
                right,
            } => {
                assert_eq!(operator.kind, TokenKind::Asterisk);
                assert!(matches!(**right, Expression::Number(3.0)));
                assert!(matches!(**left, Expression::Infix { .. }));
            }
            _ => panic!("expected infix expression"),
        }
    }

    #[test]
    fn parse_print_multiplication_has_higher_precedence_than_addition() {
        let mut parser = Parser::new(Lexer::new("BEGIN { print 1 + 2 * 3 }"));

        let program = parser.parse_program();
        let mut begin_blocks = program.begin_blocks_iter();
        let Action { statements } = begin_blocks.next().expect("expected begin block");

        let exprs = match &statements[0] {
            Statement::Print(expressions) => expressions,
            _ => panic!("expected print statement"),
        };

        match &exprs[0] {
            Expression::Infix {
                left,
                operator,
                right,
            } => {
                assert_eq!(operator.kind, TokenKind::Plus);
                assert!(matches!(**left, Expression::Number(1.0)));
                match &**right {
                    Expression::Infix {
                        operator: right_op, ..
                    } => assert_eq!(right_op.kind, TokenKind::Asterisk),
                    _ => panic!("expected nested infix expression"),
                }
            }
            _ => panic!("expected infix expression"),
        }
    }

    #[test]
    fn parse_print_power_is_right_associative() {
        let mut parser = Parser::new(Lexer::new("BEGIN { print 2 ^ 3 ^ 2 }"));

        let program = parser.parse_program();
        let mut begin_blocks = program.begin_blocks_iter();
        let Action { statements } = begin_blocks.next().expect("expected begin block");

        let exprs = match &statements[0] {
            Statement::Print(expressions) => expressions,
            _ => panic!("expected print statement"),
        };

        match &exprs[0] {
            Expression::Infix {
                left,
                operator,
                right,
            } => {
                assert_eq!(operator.kind, TokenKind::Caret);
                assert!(matches!(**left, Expression::Number(2.0)));
                match &**right {
                    Expression::Infix {
                        operator: right_op, ..
                    } => assert_eq!(right_op.kind, TokenKind::Caret),
                    _ => panic!("expected nested infix expression"),
                }
            }
            _ => panic!("expected infix expression"),
        }
    }

    #[test]
    fn parse_print_minus_is_left_associative() {
        let mut parser = Parser::new(Lexer::new("BEGIN { print 5 - 3 - 1 }"));

        let program = parser.parse_program();
        let mut begin_blocks = program.begin_blocks_iter();
        let Action { statements } = begin_blocks.next().expect("expected begin block");

        let exprs = match &statements[0] {
            Statement::Print(expressions) => expressions,
            _ => panic!("expected print statement"),
        };

        match &exprs[0] {
            Expression::Infix {
                left,
                operator,
                right,
            } => {
                assert_eq!(operator.kind, TokenKind::Minus);
                match &**left {
                    Expression::Infix {
                        operator: left_op, ..
                    } => assert_eq!(left_op.kind, TokenKind::Minus),
                    _ => panic!("expected nested infix expression"),
                }
                assert!(matches!(**right, Expression::Number(1.0)));
            }
            _ => panic!("expected infix expression"),
        }
    }

    #[test]
    fn parse_print_concatenation() {
        let mut parser = Parser::new(Lexer::new(r#"BEGIN { print "Value:" 42 }"#));

        let program = parser.parse_program();
        let mut begin_blocks = program.begin_blocks_iter();
        let Action { statements } = begin_blocks.next().expect("expected begin block");

        let exprs = match &statements[0] {
            Statement::Print(expressions) => expressions,
            _ => panic!("expected print statement"),
        };

        assert_eq!(exprs.len(), 1);
        match &exprs[0] {
            Expression::Concatenation { left, right } => {
                assert!(matches!(**left, Expression::String("Value:")));
                assert!(matches!(**right, Expression::Number(42.0)));
            }
            _ => panic!("expected concatenation expression"),
        }
    }

    #[test]
    fn parse_continue_statement() {
        let mut parser = Parser::new(Lexer::new(r#"{ continue }"#));

        let program = parser.parse_program();
        let mut rules = program.rules_iter();
        let rule = rules.next().expect("expected rule");

        let statements = match rule {
            Rule::Action(Action { statements }) => statements,
            _ => panic!("expected action rule"),
        };

        assert!(matches!(statements[0], Statement::Continue));
    }

    #[test]
    fn parse_identifier_followed_by_spaced_parentheses_as_concatenation() {
        let mut parser = Parser::new(Lexer::new(r#"{ x = $1; print x (++i) }"#));

        let program = parser.parse_program();
        let mut rules = program.rules_iter();
        let rule = rules.next().expect("expected rule");

        let statements = match rule {
            Rule::Action(Action { statements }) => statements,
            _ => panic!("expected action rule"),
        };

        let exprs = match &statements[1] {
            Statement::Print(expressions) => expressions,
            _ => panic!("expected print statement"),
        };

        assert_eq!(exprs.len(), 1);
        match &exprs[0] {
            Expression::Concatenation { left, right } => {
                assert!(matches!(**left, Expression::Identifier("x")));
                assert!(matches!(**right, Expression::PreIncrement(_)));
            }
            _ => panic!("expected concatenation expression"),
        }
    }

    #[test]
    fn parse_print_field_expression() {
        let mut parser = Parser::new(Lexer::new("{ print $1 }"));

        let program = parser.parse_program();
        let mut rules = program.rules_iter();
        let rule = rules.next().expect("expected rule");

        let statements = match rule {
            Rule::Action(Action { statements }) => statements,
            _ => panic!("expected action rule"),
        };

        let exprs = match &statements[0] {
            Statement::Print(expressions) => expressions,
            _ => panic!("expected print statement"),
        };

        match &exprs[0] {
            Expression::Field(inner) => assert!(matches!(**inner, Expression::Number(1.0))),
            _ => panic!("expected field expression"),
        }
    }

    #[test]
    fn parse_print_with_commas() {
        let mut parser = Parser::new(Lexer::new(r#"BEGIN { print "Value:", 42, $1 }"#));

        let program = parser.parse_program();

        assert_eq!(r#"BEGIN { print "Value:", 42, $1 }"#, program.to_string());
    }

    #[test]
    fn parse_number_of_fields_identifier() {
        let mut parser = Parser::new(Lexer::new(r#"BEGIN { print NF }"#));

        let program = parser.parse_program();

        assert_eq!(r#"BEGIN { print NF }"#, program.to_string());
    }

    #[test]
    fn parse_printf_with_format_and_arguments() {
        let mut parser = Parser::new(Lexer::new(r#"{ printf "[%10s] [%-16d]\n", $1, $3 }"#));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ printf "[%10s] [%-16d]\n", $1, $3 }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_print_ternary_expression() {
        let mut parser = Parser::new(Lexer::new(r#"BEGIN { print x ? y : z }"#));

        let program = parser.parse_program();
        let mut begin_blocks = program.begin_blocks_iter();
        let Action { statements } = begin_blocks.next().expect("expected begin block");

        let exprs = match &statements[0] {
            Statement::Print(expressions) => expressions,
            _ => panic!("expected print statement"),
        };

        assert_eq!(exprs.len(), 1);
        match &exprs[0] {
            Expression::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                assert!(matches!(**condition, Expression::Identifier("x")));
                assert!(matches!(**then_expr, Expression::Identifier("y")));
                assert!(matches!(**else_expr, Expression::Identifier("z")));
            }
            _ => panic!("expected ternary expression"),
        }
    }

    #[test]
    fn parse_printf_without_arguments_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new(r#"{ printf }"#));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for printf without arguments");

        assert_eq!(err.kind, ParseErrorKind::MissingPrintfFormatString);
    }

    #[test]
    fn parse_printf_without_arguments_in_parentheses_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new(r#"{ printf() }"#));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for empty printf call");

        assert_eq!(err.kind, ParseErrorKind::MissingPrintfFormatString);
    }

    #[test]
    fn parse_add_assignment_and_pre_increment() {
        let mut parser = Parser::new(Lexer::new(r#"/Asia/ { pop += $3; ++n }"#));

        let program = parser.parse_program();

        assert_eq!(r#"/Asia/ { pop += $3; ++n }"#, program.to_string());
    }

    #[test]
    fn parse_regex_match_pattern_action() {
        let mut parser = Parser::new(Lexer::new(r#"$4 ~ /Asia/ { print $1 }"#));

        let program = parser.parse_program();

        assert_eq!(r#"$4 ~ /Asia/ { print $1 }"#, program.to_string());
    }

    #[test]
    fn parse_not_pattern_action() {
        let mut parser = Parser::new(Lexer::new(r#"!($1 < 2000) { print $1 }"#));

        let program = parser.parse_program();
        let mut rules = program.rules_iter();
        let rule = rules.next().expect("expected rule");

        match rule {
            Rule::PatternAction {
                pattern: Some(Expression::Not(inner)),
                action: Some(Action { statements }),
            } => {
                assert!(matches!(**inner, Expression::Infix { .. }));
                assert!(matches!(statements[0], Statement::Print(_)));
            }
            _ => panic!("expected negated pattern action"),
        }
    }

    #[test]
    fn parse_print_with_line_continuation_after_comma() {
        let mut parser = Parser::new(Lexer::new(
            "END { print \"population of\", n,\\\n\"Asian countries in millions is\", pop }",
        ));

        let program = parser.parse_program();

        assert_eq!(
            "END { print \"population of\", n, \"Asian countries in millions is\", pop }",
            program.to_string()
        );
    }

    #[test]
    fn parse_gsub_statement() {
        let mut parser = Parser::new(Lexer::new(r#"{ gsub(/USA/, "United States"); print }"#));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ gsub(/USA/, "United States"); print }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_gsub_statement_with_target() {
        let mut parser = Parser::new(Lexer::new(r#"{ gsub(/[ \t]+/, "", t) }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ gsub(/[ \t]+/, "", t) }"#, program.to_string());
    }

    #[test]
    fn parse_system_statement() {
        let mut parser = Parser::new(Lexer::new(r#"{ system("cat " $2) }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ system("cat " $2) }"#, program.to_string());
    }

    #[test]
    fn parse_print_length_builtin_expression() {
        let mut parser = Parser::new(Lexer::new(r#"{ print length, $0 }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ print length, $0 }"#, program.to_string());
    }

    #[test]
    fn parse_length_expression_as_rule_pattern() {
        let mut parser = Parser::new(Lexer::new(
            r#"length($1) > max { max = length($1); name = $1 } END { print name }"#,
        ));

        let program = parser.parse_program();

        assert_eq!(
            r#"length($1) > max { max = length($1); name = $1 } END { print name }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_field_assignment_with_substr() {
        let mut parser = Parser::new(Lexer::new(r#"{ $1 = substr($1, 1, 3); print }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ $1 = substr($1, 1, 3); print }"#, program.to_string());
    }

    #[test]
    fn parse_assignment_with_concatenation_and_substr() {
        let mut parser = Parser::new(Lexer::new(r#"{ s = s " " substr($1, 1, 3) }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ s = s " " substr($1, 1, 3) }"#, program.to_string());
    }

    #[test]
    fn parse_field_divide_assignment() {
        let mut parser = Parser::new(Lexer::new(r#"{ $2 /= 1000; print }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ $2 = $2 / 1000; print }"#, program.to_string());
    }

    #[test]
    fn parse_chained_assignment() {
        let mut parser = Parser::new(Lexer::new(r#"BEGIN { FS = OFS = "\t" }"#));

        let program = parser.parse_program();

        assert_eq!(r#"BEGIN { FS = OFS = "\t" }"#, program.to_string());
    }

    #[test]
    fn parse_if_statement_with_block() {
        let mut parser = Parser::new(Lexer::new(
            r#"{ if (maxpop < $3) { maxpop = $3; country = $1 } }"#,
        ));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ if (maxpop < $3) { maxpop = $3; country = $1 } }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_while_with_post_increment() {
        let mut parser = Parser::new(Lexer::new(
            r#"{ i = 1; while (i <= NF) { print $i; i++ } }"#,
        ));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ i = 1; while (i <= NF) { print $i; i++ } }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_while_with_single_body_statement() {
        let mut parser = Parser::new(Lexer::new(r#"{ while (n > 1) print n }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ while (n > 1) { print n } }"#, program.to_string());
    }

    #[test]
    fn parse_do_while_with_post_increment() {
        let mut parser = Parser::new(Lexer::new(
            r#"{ i = 1; do { print $i; i++ } while (i <= NF) }"#,
        ));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ i = 1; do { print $i; i++ } while (i <= NF) }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_for_with_empty_body_statement() {
        let mut parser = Parser::new(Lexer::new(
            r#"{ for (i = 1; i <= NF; s += $(i++)) ; print s }"#,
        ));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ for (i = 1; i <= NF; s += $i++) {  }; print s }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_post_decrement_statement() {
        let mut parser = Parser::new(Lexer::new(r#"{ k-- ; n-- }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ k--; n-- }"#, program.to_string());
    }

    #[test]
    fn parse_rand_expression() {
        let mut parser = Parser::new(Lexer::new(r#"BEGIN { print rand() }"#));

        let program = parser.parse_program();

        assert_eq!(r#"BEGIN { print rand() }"#, program.to_string());
    }

    #[test]
    fn parse_math_builtin_expressions() {
        let mut parser = Parser::new(Lexer::new(
            r#"{ print log($1), sqrt($1), int(sqrt($1)), exp($1 % 10) }"#,
        ));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ print log($1), sqrt($1), int(sqrt($1)), exp($1 % 10) }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_index_builtin_expression() {
        let mut parser = Parser::new(Lexer::new(r#"{ print index(1, $1) }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ print index(1, $1) }"#, program.to_string());
    }

    #[test]
    fn parse_match_builtin_expression() {
        let mut parser = Parser::new(Lexer::new(r#"{ print match($NF, $1), RSTART, RLENGTH }"#));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ print match($NF, $1), RSTART, RLENGTH }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_in_membership_expression() {
        let mut parser = Parser::new(Lexer::new(r#"{ print 1 in x }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ print 1 in x }"#, program.to_string());
    }

    #[test]
    fn parse_parenthesized_composite_membership_expression() {
        let mut parser = Parser::new(Lexer::new(r#"{ if (($0, $1) in x) print "yes" }"#));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ if ($0, $1 in x) { print "yes" } }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_for_loop_with_single_body_statement() {
        let mut parser = Parser::new(Lexer::new(r#"{ for (i = 1; i <= NF; i++) print $i }"#));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ for (i = 1; i <= NF; i++) { print $i } }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_if_with_single_statement_body() {
        let mut parser = Parser::new(Lexer::new(
            r#"END { if (NR < 10) print FILENAME " has only " NR " lines" }"#,
        ));

        let program = parser.parse_program();

        assert_eq!(
            r#"END { if (NR < 10) { print FILENAME " has only " NR " lines" } }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_exit_statement() {
        let mut parser = Parser::new(Lexer::new(r#"NR >= 10 { exit }"#));

        let program = parser.parse_program();

        assert_eq!(r#"NR >= 10 { exit }"#, program.to_string());
    }

    #[test]
    fn parse_exit_statement_with_status() {
        let mut parser = Parser::new(Lexer::new(r#"$1 < 5000 { exit NR }"#));

        let program = parser.parse_program();

        assert_eq!(r#"$1 < 5000 { exit NR }"#, program.to_string());
    }

    #[test]
    fn parse_user_defined_function_call_statement() {
        let mut parser = Parser::new(Lexer::new(
            "BEGIN { myabort(1) }\nfunction myabort(n) { exit n }",
        ));

        let program = parser.parse_program();

        let definition = program
            .function_definition("myabort")
            .expect("expected function definition");
        assert_eq!(definition.parameters, vec!["n"]);
        assert_eq!(definition.statements.len(), 1);
    }

    #[test]
    fn parse_delete_array_element_statement() {
        let mut parser = Parser::new(Lexer::new(r#"{ delete x[i, j] }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ delete x[i, j] }"#, program.to_string());
    }

    #[test]
    fn parse_array_add_assignment_and_access() {
        let mut parser = Parser::new(Lexer::new(
            r#"/Asia/ { pop["Asia"] += $3 } END { print pop["Asia"] }"#,
        ));

        let program = parser.parse_program();

        assert_eq!(
            r#"/Asia/ { pop["Asia"] += $3 } END { print pop["Asia"] }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_for_in_loop() {
        let mut parser = Parser::new(Lexer::new(
            r#"END { for (name in area) print name ":" area[name] }"#,
        ));

        let program = parser.parse_program();

        assert_eq!(
            r#"END { for (name in area) { print name ":" area[name] } }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_print_redirection() {
        let mut parser = Parser::new(Lexer::new(r#"{ print >"tempbig" }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ print > "tempbig" }"#, program.to_string());
    }

    #[test]
    fn parse_print_pipe() {
        let mut parser = Parser::new(Lexer::new(r#"{ print c ":" pop[c] | "sort" }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ print c ":" pop[c] | "sort" }"#, program.to_string());
    }

    #[test]
    fn parse_hexadecimal_number() {
        let mut parser = Parser::new(Lexer::new(r#"BEGIN { print 0xAA }"#));

        let program = parser.parse_program();

        assert_eq!(r#"BEGIN { print 0xAA }"#, program.to_string());
    }

    #[test]
    fn parse_field_compound_assignment() {
        let mut parser = Parser::new(Lexer::new(r#"{ $1 += 2 }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ $1 = $1 + 2 }"#, program.to_string());
    }

    #[test]
    fn parse_builtin_without_parens_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new(r#"{ x = cos }"#));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for builtin used without parentheses");

        assert_eq!(err.kind, ParseErrorKind::ExpectedLeftParen);
        assert_eq!(err.token.kind, TokenKind::RightCurlyBrace);
    }

    #[test]
    fn parse_nested_function_calls() {
        let mut parser = Parser::new(Lexer::new(r#"{ x = substr(substr(s, 1, 2), 1) }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ x = substr(substr(s, 1, 2), 1) }"#, program.to_string());
    }

    #[test]
    fn parse_chained_ternary_is_right_associative() {
        let mut parser = Parser::new(Lexer::new(r#"{ x = a ? b : c ? d : e }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ x = (a) ? b : (c) ? d : e }"#, program.to_string());
    }

    #[test]
    fn parse_for_loop_with_empty_init_condition_update() {
        let mut parser = Parser::new(Lexer::new(r#"{ for (;;) break }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ for (; 1; ) { break } }"#, program.to_string());
    }

    #[test]
    fn parse_assignment_with_regex_rhs() {
        let mut parser = Parser::new(Lexer::new(r#"{ x = /foo/ }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ x = /foo/ }"#, program.to_string());
    }

    #[test]
    fn parse_field_assignment_with_regex_rhs() {
        let mut parser = Parser::new(Lexer::new(r#"{ $1 = /foo/ }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ $1 = /foo/ }"#, program.to_string());
    }

    #[test]
    fn parse_array_assignment_with_regex_rhs() {
        let mut parser = Parser::new(Lexer::new(r#"{ a[i] = /foo/ }"#));

        let program = parser.parse_program();

        assert_eq!(r#"{ a[i] = /foo/ }"#, program.to_string());
    }

    #[test]
    fn parse_for_loop_with_print_as_init_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new(r#"{ for (print "hi"; i < 10; i++) print i }"#));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for print statement as for-loop initializer");

        assert_eq!(err.kind, ParseErrorKind::UnsupportedStatement);
        assert_eq!(err.token.kind, TokenKind::Print);
    }

    #[test]
    fn parse_for_loop_with_print_as_update_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new(r#"{ for (i = 0; i < 10; print i) print i }"#));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for print statement as for-loop update");

        assert_eq!(err.kind, ParseErrorKind::UnsupportedStatement);
        assert_eq!(err.token.kind, TokenKind::Print);
    }

    #[test]
    fn parse_for_loop_with_field_assignment_as_init() {
        let mut parser = Parser::new(Lexer::new(r#"{ for ($1 = 0; $1 < 10; $1 += 1) print $1 }"#));

        let program = parser.parse_program();

        assert_eq!(
            r#"{ for ($1 = 0; $1 < 10; $1 = $1 + 1) { print $1 } }"#,
            program.to_string()
        );
    }

    #[test]
    fn parse_primary_atom_with_invalid_number_literal_returns_parse_error() {
        // The lexer always emits valid number literals, so this path is
        // unreachable through normal input. We test it directly by injecting
        // a synthetic Number token with an invalid literal.
        let mut parser = Parser::new(Lexer::new(""));
        parser.current_token = Token::new(TokenKind::Number, "0xZZ", 7);

        let err = parser
            .parse_primary_atom()
            .expect_err("expected parse error for invalid numeric literal");

        assert_eq!(err.kind, ParseErrorKind::InvalidNumericLiteral);
        assert_eq!(err.token.kind, TokenKind::Number);
        assert_eq!(err.token.literal, "0xZZ");
    }

    #[test]
    fn parse_unrecognized_token_in_expression_returns_parse_error() {
        let mut parser = Parser::new(Lexer::new("{ x = else }"));

        let err = parser
            .try_parse_program()
            .expect_err("expected parse error for unrecognized token in expression");

        assert_eq!(err.kind, ParseErrorKind::ExpectedStatement);
        assert_eq!(err.token.kind, TokenKind::Else);
    }
}
