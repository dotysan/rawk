use crate::{
    Action, Program, Rule,
    ast::{Expression, Statement},
    token::TokenKind,
};
use regex::Regex;
use std::cell::Cell;
use std::collections::HashMap;

struct FunctionCallResult {
    value: String,
    output: Vec<String>,
}

struct ComparisonOperand {
    text: String,
    numeric: Option<f64>,
}

pub struct Evaluator<'a> {
    program: Program<'a>,
    input_iter: Box<dyn Iterator<Item = String>>,
    input_cursor: usize,
    current_line_number: Cell<usize>,
    current_line: Option<String>,
    field_separator: String,
    output_field_separator: String,
    output_record_separator: String,
    current_filename: String,
    variables: HashMap<String, String>,
    numeric_variables: HashMap<String, f64>,
    array_variables: HashMap<String, String>,
    array_aliases: HashMap<String, String>,
    argv: Vec<String>,
    pipe_outputs: HashMap<String, Vec<String>>,
    rng_state: Cell<u64>,
    printf_buffer: String,
    expression_output: Vec<String>,
    exited: bool,
    next_record: bool,
    break_loop: bool,
    continue_loop: bool,
    return_value: Option<String>,
    has_output: bool,
    runtime_error: Option<String>,
}

impl<'a> Evaluator<'a> {
    pub fn new(
        program: Program<'a>,
        input: impl IntoIterator<Item = String> + 'static,
        current_filename: impl Into<String>,
    ) -> Self {
        let current_filename = current_filename.into();
        Self {
            program,
            input_iter: Box::new(input.into_iter()),
            input_cursor: 0,
            current_line_number: Cell::new(0),
            current_line: None,
            field_separator: " ".to_string(),
            output_field_separator: " ".to_string(),
            output_record_separator: "\n".to_string(),
            current_filename: current_filename.clone(),
            variables: HashMap::new(),
            numeric_variables: HashMap::new(),
            array_variables: HashMap::new(),
            array_aliases: HashMap::new(),
            argv: vec!["rawk".to_string(), current_filename],
            pipe_outputs: HashMap::new(),
            rng_state: Cell::new(9),
            printf_buffer: String::new(),
            expression_output: Vec::new(),
            exited: false,
            next_record: false,
            break_loop: false,
            continue_loop: false,
            return_value: None,
            has_output: false,
            runtime_error: None,
        }
    }

    pub fn with_field_separator(mut self, fs: String) -> Self {
        self.field_separator = fs;
        self
    }

    pub fn runtime_error(&self) -> Option<&str> {
        self.runtime_error.as_deref()
    }

    pub fn eval(&mut self) -> Vec<String> {
        let mut output_lines: Vec<String> = Vec::new();

        let begin_actions: Vec<Action<'a>> = self.program.begin_blocks_iter().cloned().collect();
        for action in begin_actions.iter() {
            output_lines.extend(self.eval_action(action, None));
            if self.exited || self.runtime_error.is_some() {
                break;
            }
        }

        if self.runtime_error.is_some() {
            return normalize_output_lines(output_lines);
        }

        let rules: Vec<Rule<'a>> = self.program.rules_iter().cloned().collect();
        let mut range_state = vec![false; rules.len()];
        while let Some(input_line) = self.read_next_input_record() {
            if self.exited {
                break;
            }

            for (rule_idx, rule) in rules.iter().enumerate() {
                if self.exited || self.runtime_error.is_some() {
                    break;
                }
                // Rules are evaluated against the original record text for this
                // interpreter's current semantics.
                output_lines.extend(self.eval_rule_for_line(
                    rule,
                    &input_line,
                    &mut range_state[rule_idx],
                ));
                if self.next_record {
                    self.next_record = false;
                    break;
                }
            }
            if self.runtime_error.is_some() {
                break;
            }
        }

        if self.runtime_error.is_some() {
            return normalize_output_lines(output_lines);
        }

        if !self.exited {
            self.current_line_number.set(self.input_cursor);
        }
        self.current_line = None;

        let end_actions: Vec<Action<'a>> = self.program.end_blocks_iter().cloned().collect();
        self.exited = false;
        for action in end_actions.iter() {
            output_lines.extend(self.eval_action(action, None));
            if self.exited {
                break;
            }
        }

        if !self.printf_buffer.is_empty() {
            let pending_printf = std::mem::take(&mut self.printf_buffer);
            self.append_generated_output(&mut output_lines, vec![pending_printf]);
        }

        for line in self.flush_pipe_outputs() {
            self.append_generated_output(&mut output_lines, vec![line]);
        }

        normalize_output_lines(output_lines)
    }

    fn read_next_input_record(&mut self) -> Option<String> {
        let input_line = self.input_iter.next()?;
        self.input_cursor += 1;
        self.current_line_number.set(self.input_cursor);
        self.current_line = Some(input_line.clone());
        self.variables.remove("NF");
        self.numeric_variables.remove("NF");
        Some(input_line)
    }

    fn eval_rule_for_line(
        &mut self,
        rule: &Rule,
        input_line: &str,
        range_active: &mut bool,
    ) -> Vec<String> {
        match rule {
            Rule::Action(action) => self.eval_action(action, Some(input_line)),
            Rule::PatternAction { pattern, action } => {
                let matches = match pattern.as_ref() {
                    Some(expr) => self.eval_pattern_condition(expr, range_active),
                    None => true,
                };
                if !matches {
                    return Vec::new();
                }

                if let Some(action) = action {
                    self.eval_action(action, Some(input_line))
                } else {
                    let mut output = Vec::new();
                    output.push(input_line.to_string());
                    self.append_output_record_separator(&mut output);
                    output
                }
            }
            _ => Vec::new(),
        }
    }

    fn eval_pattern_condition(
        &mut self,
        expression: &Expression<'_>,
        range_active: &mut bool,
    ) -> bool {
        if let Expression::Infix {
            left,
            operator,
            right,
        } = expression
            && operator.kind == TokenKind::Comma
        {
            if !*range_active {
                let start = self.eval_condition(left);
                if !start {
                    return false;
                }
                *range_active = true;
            }

            let matched = true;
            if self.eval_condition(right) {
                *range_active = false;
            }
            return matched;
        }

        self.eval_condition(expression)
    }

    fn should_break_statement_sequence(&self) -> bool {
        self.exited
            || self.next_record
            || self.break_loop
            || self.continue_loop
            || self.return_value.is_some()
            || self.runtime_error.is_some()
    }

    fn should_break_loop_iteration(&self) -> bool {
        self.exited || self.next_record || self.break_loop || self.return_value.is_some()
    }

    fn should_break_function_body(&self) -> bool {
        self.exited || self.next_record || self.return_value.is_some()
    }

    fn clear_loop_control_flags(&mut self) {
        self.break_loop = false;
        self.continue_loop = false;
    }

    fn eval_statement_block(
        &mut self,
        statements: &[Statement<'_>],
        input_line: Option<&str>,
    ) -> Vec<String> {
        let mut output = Vec::new();

        for statement in statements {
            let statement_output = self.eval_statement(statement, input_line);
            self.append_local_output(&mut output, statement_output);
            if self.should_break_statement_sequence() {
                break;
            }
        }

        output
    }

    fn eval_action(&mut self, action: &Action, input_line: Option<&str>) -> Vec<String> {
        let mut output = Vec::new();

        for statement in &action.statements {
            let statement_output = self.eval_statement(statement, input_line);
            if statement_output.is_empty() {
                if self.should_break_statement_sequence() {
                    break;
                }
                continue;
            }
            self.append_generated_output(&mut output, statement_output);
            if self.should_break_statement_sequence() {
                break;
            }
        }

        output
    }

    fn append_generated_output(&mut self, output: &mut Vec<String>, generated: Vec<String>) {
        if generated.is_empty() {
            return;
        }
        output.extend(generated);
        self.has_output = true;
    }

    fn append_local_output(&self, output: &mut Vec<String>, generated: Vec<String>) {
        if generated.is_empty() {
            return;
        }
        output.extend(generated);
    }

    fn eval_statement(
        &mut self,
        statement: &Statement<'_>,
        input_line: Option<&str>,
    ) -> Vec<String> {
        let output = match statement {
            Statement::Empty => Vec::new(),
            Statement::Expression(expression) => match expression {
                Expression::FunctionCall { name, args } => {
                    self.eval_user_defined_function_call(name, args).output
                }
                _ => {
                    let _ = self.eval_expression(expression);
                    Vec::new()
                }
            },
            Statement::Print(expressions) => self.eval_print_statement(expressions, input_line),
            Statement::PrintPipe {
                expressions,
                target,
            } => {
                self.eval_print_pipe(expressions, target, input_line);
                Vec::new()
            }
            Statement::PrintRedirect {
                expressions,
                target,
                append,
            } => {
                self.eval_print_redirect(expressions, target, *append, input_line);
                Vec::new()
            }
            Statement::Printf(expressions) => self.eval_printf_statement(expressions),
            Statement::System(command) => {
                self.eval_system(command);
                Vec::new()
            }
            Statement::Split {
                string,
                array,
                separator,
            } => {
                self.eval_split(string, array, separator.as_ref());
                Vec::new()
            }
            Statement::Sub {
                pattern,
                replacement,
            } => {
                self.eval_sub(pattern, replacement);
                Vec::new()
            }
            Statement::Gsub {
                pattern,
                replacement,
                target,
            } => {
                self.eval_gsub(pattern, replacement, target.as_ref());
                Vec::new()
            }
            Statement::Assignment { identifier, value } => {
                self.eval_assignment(identifier, value);
                Vec::new()
            }
            Statement::SplitAssignment {
                identifier,
                string,
                array,
                separator,
            } => {
                self.eval_split_assignment(identifier, string, array, separator.as_ref());
                Vec::new()
            }
            Statement::ArrayAssignment {
                identifier,
                index,
                value,
            } => {
                self.eval_array_assignment(identifier, index, value);
                Vec::new()
            }
            Statement::FieldAssignment { field, value } => {
                self.eval_field_assignment(field, value);
                Vec::new()
            }
            Statement::AddAssignment { identifier, value } => {
                self.eval_add_assignment(identifier, value);
                Vec::new()
            }
            Statement::ArrayAddAssignment {
                identifier,
                index,
                value,
            } => {
                self.eval_array_add_assignment(identifier, index, value);
                Vec::new()
            }
            Statement::ArrayPostIncrement { identifier, index } => {
                self.eval_array_post_increment(identifier, index, 1.0);
                Vec::new()
            }
            Statement::ArrayPostDecrement { identifier, index } => {
                self.eval_array_post_increment(identifier, index, -1.0);
                Vec::new()
            }
            Statement::Delete { identifier, index } => {
                self.eval_delete(identifier, index.as_ref());
                Vec::new()
            }
            Statement::PreIncrement { identifier } => {
                self.eval_pre_increment(identifier);
                Vec::new()
            }
            Statement::PreDecrement { identifier } => {
                self.eval_pre_decrement(identifier);
                Vec::new()
            }
            Statement::PostIncrement { identifier } => {
                self.eval_post_increment(identifier);
                Vec::new()
            }
            Statement::PostDecrement { identifier } => {
                self.eval_post_decrement(identifier);
                Vec::new()
            }
            Statement::If {
                condition,
                then_statements,
            } => {
                if self.eval_condition(condition) {
                    let mut output = Vec::new();
                    for statement in then_statements {
                        let statement_output = self.eval_statement(statement, input_line);
                        self.append_local_output(&mut output, statement_output);
                        if self.should_break_statement_sequence() {
                            break;
                        }
                    }
                    output
                } else {
                    Vec::new()
                }
            }
            Statement::IfElse {
                condition,
                then_statements,
                else_statements,
            } => {
                let branch = if self.eval_condition(condition) {
                    then_statements
                } else {
                    else_statements
                };
                let mut output = Vec::new();
                for statement in branch {
                    let statement_output = self.eval_statement(statement, input_line);
                    self.append_local_output(&mut output, statement_output);
                    if self.should_break_statement_sequence() {
                        break;
                    }
                }
                output
            }
            Statement::While {
                condition,
                statements,
            } => {
                let mut output = Vec::new();
                while self.eval_condition(condition) {
                    let statement_output = self.eval_statement_block(statements, input_line);
                    self.append_local_output(&mut output, statement_output);
                    if self.should_break_loop_iteration() {
                        break;
                    }
                    if self.continue_loop {
                        self.continue_loop = false;
                    }
                }
                self.clear_loop_control_flags();
                output
            }
            Statement::DoWhile {
                condition,
                statements,
            } => {
                let mut output = Vec::new();
                loop {
                    let statement_output = self.eval_statement_block(statements, input_line);
                    self.append_local_output(&mut output, statement_output);
                    if self.should_break_loop_iteration() {
                        break;
                    }
                    if self.continue_loop {
                        self.continue_loop = false;
                    }
                    if !self.eval_condition(condition) {
                        break;
                    }
                }
                self.clear_loop_control_flags();
                output
            }
            Statement::For {
                init,
                condition,
                update,
                statements,
            } => {
                let mut output = Vec::new();
                let init_output = self.eval_statement(init, input_line);
                self.append_local_output(&mut output, init_output);
                if self.should_break_statement_sequence() {
                    return output;
                }
                while self.eval_condition(condition) {
                    for statement in statements {
                        let statement_output = self.eval_statement(statement, input_line);
                        self.append_local_output(&mut output, statement_output);
                        if self.should_break_statement_sequence() {
                            break;
                        }
                    }
                    if self.should_break_loop_iteration() {
                        break;
                    }
                    let update_output = self.eval_statement(update, input_line);
                    self.append_local_output(&mut output, update_output);
                    if self.continue_loop {
                        self.continue_loop = false;
                    }
                    if self.should_break_loop_iteration() {
                        break;
                    }
                }
                self.clear_loop_control_flags();
                output
            }
            Statement::ForIn {
                variable,
                array,
                statements,
            } => {
                let mut keys = self.array_keys(array);
                keys.sort();
                let mut output = Vec::new();
                for key in keys {
                    self.set_variable_text(variable, key);
                    for statement in statements {
                        let statement_output = self.eval_statement(statement, input_line);
                        self.append_local_output(&mut output, statement_output);
                        if self.should_break_statement_sequence() {
                            break;
                        }
                    }
                    if self.should_break_loop_iteration() {
                        break;
                    }
                    if self.continue_loop {
                        self.continue_loop = false;
                        continue;
                    }
                }
                self.clear_loop_control_flags();
                output
            }
            Statement::Break => {
                self.break_loop = true;
                Vec::new()
            }
            Statement::Continue => {
                self.continue_loop = true;
                Vec::new()
            }
            Statement::Return(value) => {
                self.return_value = Some(
                    value
                        .as_ref()
                        .map(|value| self.eval_expression(value))
                        .unwrap_or_default(),
                );
                Vec::new()
            }
            Statement::Next => {
                self.next_record = true;
                Vec::new()
            }
            Statement::Exit(status) => {
                if let Some(status) = status {
                    let _ = self.eval_expression(status);
                }
                self.exited = true;
                Vec::new()
            }
        };

        let mut expression_output = self.take_expression_output();
        expression_output.extend(output);
        expression_output
    }

    fn eval_print(&mut self, expressions: &[Expression<'_>], input_line: Option<&str>) -> String {
        if expressions.is_empty() {
            return self
                .current_line
                .as_deref()
                .or(input_line)
                .unwrap_or("")
                .to_string();
        }

        let parts = expressions
            .iter()
            .map(|expr| self.eval_expression(expr))
            .collect::<Vec<String>>();
        parts.join(&self.output_field_separator)
    }

    fn eval_print_statement(
        &mut self,
        expressions: &[Expression<'_>],
        input_line: Option<&str>,
    ) -> Vec<String> {
        let pending_printf = std::mem::take(&mut self.printf_buffer);
        if expressions.is_empty() {
            let line = self
                .current_line
                .as_deref()
                .or(input_line)
                .unwrap_or("")
                .to_string();
            let mut output = vec![format!("{pending_printf}{line}")];
            self.append_output_record_separator(&mut output);
            return output;
        }

        let mut output = Vec::new();
        let mut parts = Vec::new();
        for expression in expressions {
            parts.push(self.eval_expression(expression));
            output.extend(self.take_expression_output());
            if self.runtime_error.is_some() {
                return Vec::new();
            }
        }
        let rendered = format!(
            "{pending_printf}{}",
            parts.join(&self.output_field_separator)
        );
        output.push(rendered);
        self.append_output_record_separator(&mut output);
        output
    }

    fn take_expression_output(&mut self) -> Vec<String> {
        std::mem::take(&mut self.expression_output)
    }

    fn eval_printf(&mut self, expressions: &[Expression<'_>]) -> String {
        if expressions.is_empty() {
            self.runtime_error = Some("printf requires at least a format string".to_string());
            return String::new();
        }

        let format = self.eval_expression(&expressions[0]);
        let format = unescape_awk_string(&format);
        let args: Vec<String> = expressions
            .iter()
            .skip(1)
            .map(|expr| unescape_awk_string(&self.eval_printf_argument(expr)))
            .collect();

        format_printf(&format, &args)
    }

    fn eval_printf_argument(&mut self, expression: &Expression<'_>) -> String {
        match expression {
            Expression::Number(_)
            | Expression::HexNumber { .. }
            | Expression::Infix { .. }
            | Expression::Rand
            | Expression::Length(_) => self
                .eval_numeric_expression(expression)
                .map(|value| value.to_string())
                .unwrap_or_else(|| self.eval_expression(expression)),
            _ => self.eval_expression(expression),
        }
    }

    fn eval_printf_statement(&mut self, expressions: &[Expression<'_>]) -> Vec<String> {
        let rendered = self.eval_printf(expressions);
        if rendered.is_empty() {
            return Vec::new();
        }
        self.printf_buffer.push_str(&rendered);
        let mut output = Vec::new();
        while let Some(index) = self.printf_buffer.find('\n') {
            output.push(self.printf_buffer[..index].trim_end().to_string());
            output.push("\n".to_string());
            self.printf_buffer = self.printf_buffer[index + 1..].to_string();
        }
        output
    }

    fn eval_print_redirect(
        &mut self,
        expressions: &[Expression<'_>],
        target: &Expression<'_>,
        _append: bool,
        input_line: Option<&str>,
    ) {
        let _rendered = self.eval_print(expressions, input_line);
        let _target = self.eval_expression(target);
    }

    fn eval_print_pipe(
        &mut self,
        expressions: &[Expression<'_>],
        target: &Expression<'_>,
        input_line: Option<&str>,
    ) {
        let mut rendered = self.eval_print(expressions, input_line);
        rendered.push_str(&self.output_record_separator);
        let target = self.eval_expression(target);
        self.pipe_outputs.entry(target).or_default().push(rendered);
    }

    fn eval_assignment(&mut self, identifier: &str, value: &Expression<'_>) {
        match value {
            Expression::String(value) => {
                self.set_variable_text(identifier, unescape_awk_string(value))
            }
            Expression::Infix {
                left,
                operator,
                right,
            } if operator.kind == TokenKind::Assign => {
                let assigned_value = self.eval_assignment_infix(left, right);
                self.set_variable_text(identifier, assigned_value);
            }
            _ if expression_has_precise_numeric_value(value) => {
                let assigned_value = self.eval_numeric_expression(value).unwrap_or(0.0);
                self.set_variable_numeric(identifier, assigned_value);
            }
            _ => {
                let assigned_value = self.eval_expression(value);
                self.set_variable_text(identifier, assigned_value);
            }
        }
    }

    fn eval_add_assignment(&mut self, identifier: &str, value: &Expression<'_>) {
        let current = parse_awk_numeric(&self.eval_identifier_expression(identifier));
        let increment = self.eval_numeric_expression(value).unwrap_or(0.0);
        self.set_variable_numeric(identifier, current + increment);
    }

    fn eval_array_assignment(
        &mut self,
        identifier: &str,
        index: &Expression<'_>,
        value: &Expression<'_>,
    ) {
        let key = self.array_key(identifier, index);
        let assigned_value = self.eval_expression(value);
        self.array_variables.insert(key, assigned_value);
    }

    fn eval_array_add_assignment(
        &mut self,
        identifier: &str,
        index: &Expression<'_>,
        value: &Expression<'_>,
    ) {
        let key = self.array_key(identifier, index);
        let current = self
            .array_variables
            .get(&key)
            .map(|value| parse_awk_numeric(value))
            .unwrap_or(0.0);
        let increment = self.eval_numeric_expression(value).unwrap_or(0.0);
        self.array_variables
            .insert(key, (current + increment).to_string());
    }

    fn eval_split_assignment(
        &mut self,
        identifier: &str,
        string: &Expression<'_>,
        array: &str,
        separator: Option<&Expression<'_>>,
    ) {
        let count = self.eval_split(string, array, separator);
        self.set_variable_numeric(identifier, count as f64);
    }

    fn eval_split(
        &mut self,
        string: &Expression<'_>,
        array: &str,
        separator: Option<&Expression<'_>>,
    ) -> usize {
        let source = self.eval_expression(string);
        let array = self.resolve_array_identifier(array).to_string();
        let fields = self.split_source(&source, separator);
        let prefix = format!("{array}\u{1f}");
        self.array_variables
            .retain(|key, _| !key.starts_with(&prefix));
        for (idx, value) in fields.iter().enumerate() {
            let key = format!("{array}\u{1f}{}", idx + 1);
            self.array_variables.insert(key, value.clone());
        }
        fields.len()
    }

    fn eval_array_post_increment(&mut self, identifier: &str, index: &Expression<'_>, delta: f64) {
        let key = self.array_key(identifier, index);
        let current = self
            .array_variables
            .get(&key)
            .map(|value| parse_awk_numeric(value))
            .unwrap_or(0.0);
        self.array_variables
            .insert(key, format_awk_number(current + delta));
    }

    fn eval_delete(&mut self, identifier: &str, index: Option<&Expression<'_>>) {
        if let Some(index) = index {
            let key = self.array_key(identifier, index);
            self.array_variables.remove(&key);
            return;
        }

        let identifier = self.resolve_array_identifier(identifier);
        let prefix = format!("{identifier}\u{1f}");
        self.array_variables
            .retain(|key, _| !key.starts_with(&prefix));
    }

    fn eval_field_assignment(&mut self, field: &Expression<'_>, value: &Expression<'_>) {
        let assigned_value = self.eval_expression(value);
        self.assign_field(field, assigned_value);
    }

    fn assign_field(&mut self, field: &Expression<'_>, value: String) {
        let line = match self.current_line.as_ref() {
            Some(value) => value.clone(),
            None => return,
        };

        let index = self.eval_numeric_expression(field).unwrap_or(0.0) as i64;
        if index == 0 {
            self.current_line = Some(value);
            return;
        }
        if index < 0 {
            return;
        }

        let mut fields = self.split_line_into_fields(&line);
        while fields.len() < index as usize {
            fields.push(String::new());
        }
        fields[(index - 1) as usize] = value;
        self.set_variable_numeric("NF", fields.len() as f64);
        self.current_line = Some(fields.join(&self.output_field_separator));
    }

    fn eval_assignment_infix(&mut self, left: &Expression<'_>, right: &Expression<'_>) -> String {
        let assigned_value = if let Expression::String(value) = right {
            unescape_awk_string(value)
        } else if let Expression::Infix {
            left: nested_left,
            operator: nested_operator,
            right: nested_right,
        } = right
        {
            if nested_operator.kind == TokenKind::Assign {
                self.eval_assignment_infix(nested_left, nested_right)
            } else {
                self.eval_expression(right)
            }
        } else {
            self.eval_expression(right)
        };

        match left {
            Expression::Identifier(identifier) => {
                if expression_has_precise_numeric_value(right) {
                    let assigned_numeric = self.eval_numeric_expression(right).unwrap_or(0.0);
                    self.set_variable_numeric(identifier, assigned_numeric);
                    format_awk_number(assigned_numeric)
                } else {
                    self.set_variable_text(identifier, assigned_value.clone());
                    assigned_value
                }
            }
            Expression::Field(field) => {
                self.assign_field(field, assigned_value.clone());
                assigned_value
            }
            _ => assigned_value,
        }
    }

    fn set_special_variable(&mut self, identifier: &str, value: &str) {
        if identifier == "FS" {
            self.field_separator = unescape_awk_string(value);
        } else if identifier == "OFS" {
            self.output_field_separator = unescape_awk_string(value);
        } else if identifier == "ORS" {
            self.output_record_separator = unescape_awk_string(value);
        } else if identifier == "NF" {
            self.set_number_of_fields(value);
        }
    }

    fn set_number_of_fields(&mut self, value: &str) {
        let line = match self.current_line.as_ref() {
            Some(value) => value.clone(),
            None => return,
        };

        let target_nf = parse_awk_numeric(value).trunc().max(0.0) as usize;
        let mut fields = self.split_fields(&line);
        if fields.len() > target_nf {
            fields.truncate(target_nf);
        } else {
            while fields.len() < target_nf {
                fields.push(String::new());
            }
        }

        self.current_line = Some(fields.join(&self.output_field_separator));
        self.variables
            .insert("NF".to_string(), target_nf.to_string());
        self.numeric_variables
            .insert("NF".to_string(), target_nf as f64);
    }

    fn append_output_record_separator(&self, output: &mut Vec<String>) {
        if self.output_record_separator.is_empty() {
            return;
        }
        output.push(self.output_record_separator.clone());
    }

    fn flush_pipe_outputs(&mut self) -> Vec<String> {
        let mut output = Vec::new();
        let mut keys: Vec<String> = self.pipe_outputs.keys().cloned().collect();
        keys.sort();

        for key in keys {
            let mut lines = self.pipe_outputs.remove(&key).unwrap_or_default();
            if key == "sort" {
                lines.sort();
            }
            output.extend(lines);
        }

        output
    }

    fn eval_pre_increment(&mut self, identifier: &str) {
        let current = parse_awk_numeric(&self.eval_identifier_expression(identifier));
        self.set_variable_numeric(identifier, current + 1.0);
    }

    fn eval_pre_decrement(&mut self, identifier: &str) {
        let current = parse_awk_numeric(&self.eval_identifier_expression(identifier));
        self.set_variable_numeric(identifier, current - 1.0);
    }

    fn eval_post_increment(&mut self, identifier: &str) {
        let current = parse_awk_numeric(&self.eval_identifier_expression(identifier));
        self.set_variable_numeric(identifier, current + 1.0);
    }

    fn eval_post_decrement(&mut self, identifier: &str) {
        let current = parse_awk_numeric(&self.eval_identifier_expression(identifier));
        self.set_variable_numeric(identifier, current - 1.0);
    }

    fn eval_gsub(
        &mut self,
        pattern: &Expression<'_>,
        replacement: &Expression<'_>,
        target: Option<&Expression<'_>>,
    ) {
        let pattern = match pattern {
            Expression::Regex(value) => value.to_string(),
            _ => self.eval_expression(pattern),
        };
        if let Err(err) = Regex::new(&pattern) {
            self.runtime_error = Some(format!("invalid regex in gsub: {err}"));
            return;
        }
        let replacement = unescape_awk_string(&self.eval_expression(replacement));
        match target {
            Some(Expression::Identifier(identifier)) => {
                let value = self.eval_identifier_expression(identifier);
                let replaced = awk_gsub_replace_all(&value, &pattern, &replacement);
                self.set_variable_text(identifier, replaced);
            }
            Some(Expression::Field(inner)) => {
                let line = self.eval_field_expression(inner);
                let replaced = awk_gsub_replace_all(&line, &pattern, &replacement);
                self.assign_field(inner, replaced);
            }
            Some(other) => {
                let value = self.eval_expression(other);
                let _ = awk_gsub_replace_all(&value, &pattern, &replacement);
            }
            None => {
                let line = match self.current_line.as_ref() {
                    Some(value) => value.clone(),
                    None => return,
                };
                let replaced = awk_gsub_replace_all(&line, &pattern, &replacement);
                self.current_line = Some(replaced);
            }
        }
    }

    fn eval_sub(&mut self, pattern: &Expression<'_>, replacement: &Expression<'_>) {
        let line = match self.current_line.as_ref() {
            Some(value) => value.clone(),
            None => return,
        };

        let pattern = match pattern {
            Expression::Regex(value) => value.to_string(),
            _ => self.eval_expression(pattern),
        };
        if let Err(err) = Regex::new(&pattern) {
            self.runtime_error = Some(format!("invalid regex in sub: {err}"));
            return;
        }
        let replacement = unescape_awk_string(&self.eval_expression(replacement));
        let replaced = awk_sub_replace_first(&line, &pattern, &replacement);
        self.current_line = Some(replaced);
    }

    fn eval_system(&mut self, command: &Expression<'_>) {
        let _command = self.eval_expression(command);
    }

    fn eval_expression(&mut self, expression: &Expression) -> String {
        match expression {
            Expression::String(value) => unescape_awk_string(value),
            Expression::Number(value) => format_awk_number(*value),
            Expression::HexNumber { value, .. } => format_awk_number(*value),
            Expression::Regex(value) => value.to_string(),
            Expression::Field(inner) => self.eval_field_expression(inner),
            Expression::Identifier(identifier) => self.eval_identifier_expression(identifier),
            Expression::ArrayAccess { identifier, index } => {
                self.eval_array_access(identifier, index)
            }
            Expression::Length(expression) => self.eval_length_expression(expression.as_deref()),
            Expression::Substr {
                string,
                start,
                length,
            } => self.eval_substr_expression(string, start, length.as_deref()),
            Expression::Rand => format_awk_number(self.eval_rand()),
            Expression::FunctionCall { name, args } => {
                let result = self.eval_user_defined_function_call(name, args);
                self.expression_output.extend(result.output);
                result.value
            }
            Expression::Not(expression) => {
                if self.eval_condition(expression) {
                    "0".to_string()
                } else {
                    "1".to_string()
                }
            }
            Expression::PreIncrement(target) => self.eval_increment_expression(target, 1.0, true),
            Expression::PreDecrement(target) => self.eval_increment_expression(target, -1.0, true),
            Expression::PostIncrement(target) => self.eval_increment_expression(target, 1.0, false),
            Expression::PostDecrement(target) => {
                self.eval_increment_expression(target, -1.0, false)
            }
            Expression::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                if self.eval_condition(condition) {
                    self.eval_expression(then_expr)
                } else {
                    self.eval_expression(else_expr)
                }
            }
            Expression::Concatenation { left, right } => {
                let mut value = self.eval_expression(left);
                value.push_str(&self.eval_expression(right));
                value
            }
            Expression::Infix {
                left,
                operator,
                right,
            } => {
                if operator.kind == TokenKind::Assign {
                    self.eval_assignment_infix(left, right)
                } else if let Some(value) =
                    self.eval_regex_match(left, operator.kind.clone(), right)
                {
                    format_awk_number(if value { 1.0 } else { 0.0 })
                } else if let Some(value) = self.eval_membership(left, operator.kind.clone(), right)
                {
                    format_awk_number(if value { 1.0 } else { 0.0 })
                } else {
                    match self.eval_numeric_infix(left, operator, right) {
                        Some(value) => format_awk_number(value),
                        None if self.runtime_error.is_none() => {
                            self.runtime_error = Some(format!(
                                "operator '{}' cannot be applied to given operands",
                                operator.literal
                            ));
                            String::new()
                        }
                        None => String::new(),
                    }
                }
            }
        }
    }

    fn eval_increment_expression(
        &mut self,
        target: &Expression<'_>,
        delta: f64,
        return_new: bool,
    ) -> String {
        match target {
            Expression::Identifier(identifier) => {
                let current = parse_awk_numeric(&self.eval_identifier_expression(identifier));
                let updated = current + delta;
                self.set_variable_numeric(identifier, updated);
                if return_new {
                    format_awk_number(updated)
                } else {
                    format_awk_number(current)
                }
            }
            Expression::Field(field) => {
                let current = parse_awk_numeric(&self.eval_field_expression(field));
                let updated = current + delta;
                self.assign_field(field, format_awk_number(updated));
                if return_new {
                    format_awk_number(updated)
                } else {
                    format_awk_number(current)
                }
            }
            _ => "0".to_string(),
        }
    }

    fn eval_identifier_expression(&mut self, identifier: &str) -> String {
        match identifier {
            "getline" => self.eval_getline(),
            "FS" => self.field_separator.clone(),
            "OFS" => self.output_field_separator.clone(),
            "ORS" => self.output_record_separator.clone(),
            "NF" => {
                if let Some(value) = self.variables.get("NF") {
                    return value.clone();
                }
                let line = match self.current_line.as_deref() {
                    Some(value) => value,
                    None => return "0".to_string(),
                };
                let field_count = self.split_line_into_fields(line).len();
                field_count.to_string()
            }
            "NR" => match self.current_line.as_ref() {
                Some(_) => self.current_line_number.get().to_string(),
                None => self.current_line_number.get().to_string(),
            },
            "FNR" => self.current_line_number.get().to_string(),
            "FILENAME" => {
                if self.current_line.is_none() && self.current_line_number.get() == 0 {
                    String::new()
                } else {
                    self.current_filename.clone()
                }
            }
            "ARGC" => self.argv.len().to_string(),
            _ => self.variables.get(identifier).cloned().unwrap_or_default(),
        }
    }

    fn set_variable_text(&mut self, identifier: &str, value: String) {
        if identifier == "NF" {
            self.set_number_of_fields(&value);
            return;
        }
        self.set_special_variable(identifier, &value);
        self.variables.insert(identifier.to_string(), value.clone());
        if let Some(numeric) = parse_full_awk_numeric(&value) {
            self.numeric_variables
                .insert(identifier.to_string(), numeric);
        } else {
            self.numeric_variables.remove(identifier);
        }
    }

    fn set_variable_numeric(&mut self, identifier: &str, value: f64) {
        let rendered = format_awk_number(value);
        if identifier == "NF" {
            self.set_number_of_fields(&rendered);
            return;
        }
        self.set_special_variable(identifier, &rendered);
        self.variables.insert(identifier.to_string(), rendered);
        self.numeric_variables.insert(identifier.to_string(), value);
    }

    fn eval_getline(&mut self) -> String {
        if self.read_next_input_record().is_some() {
            "1".to_string()
        } else {
            self.current_line = None;
            "0".to_string()
        }
    }

    fn array_key(&mut self, identifier: &str, index: &Expression<'_>) -> String {
        let identifier = self.resolve_array_identifier(identifier).to_string();
        format!("{identifier}\u{1f}{}", self.eval_array_subscript(index))
    }

    fn resolve_array_identifier<'b>(&'b self, identifier: &'b str) -> &'b str {
        self.array_aliases
            .get(identifier)
            .map(|alias| alias.as_str())
            .unwrap_or(identifier)
    }

    fn eval_array_subscript(&mut self, index: &Expression<'_>) -> String {
        match index {
            Expression::Infix {
                left,
                operator,
                right,
            } if operator.kind == TokenKind::Comma => {
                let mut value = self.eval_array_subscript(left);
                value.push('\u{1c}');
                value.push_str(&self.eval_array_subscript(right));
                value
            }
            _ => self.eval_expression(index),
        }
    }

    fn eval_array_access(&mut self, identifier: &str, index: &Expression<'_>) -> String {
        if identifier == "ARGV" {
            let idx = self.eval_numeric_expression(index).unwrap_or(0.0) as isize;
            if idx < 0 {
                return String::new();
            }
            return self.argv.get(idx as usize).cloned().unwrap_or_default();
        }

        let key = self.array_key(identifier, index);
        self.array_variables.get(&key).cloned().unwrap_or_default()
    }

    fn array_keys(&self, identifier: &str) -> Vec<String> {
        let identifier = self.resolve_array_identifier(identifier);
        let prefix = format!("{identifier}\u{1f}");
        self.array_variables
            .keys()
            .filter_map(|key| key.strip_prefix(&prefix).map(str::to_string))
            .collect()
    }

    fn eval_field_expression(&mut self, expression: &Expression<'_>) -> String {
        let index = match self.eval_numeric_expression(expression) {
            Some(value) => value as i64,
            None => return String::new(),
        };

        let line = match self.current_line.as_deref() {
            Some(value) => value,
            None => return String::new(),
        };

        if index == 0 {
            return line.to_string();
        }

        if index < 0 {
            self.runtime_error = Some(format!("attempt to access field {index}"));
            return String::new();
        }

        self.split_line_into_fields(line)
            .into_iter()
            .nth((index - 1) as usize)
            .unwrap_or_default()
    }

    fn eval_length_expression(&mut self, expression: Option<&Expression<'_>>) -> String {
        let value = match expression {
            Some(expr) => self.eval_expression(expr),
            None => self.current_line.clone().unwrap_or_default(),
        };
        value.chars().count().to_string()
    }

    fn eval_substr_expression(
        &mut self,
        string: &Expression<'_>,
        start: &Expression<'_>,
        length: Option<&Expression<'_>>,
    ) -> String {
        let source = self.eval_expression(string);
        let chars: Vec<char> = source.chars().collect();
        if chars.is_empty() {
            return String::new();
        }

        let start_index = self.eval_numeric_expression(start).unwrap_or(1.0) as i64;
        let start_index = if start_index <= 1 {
            0
        } else {
            (start_index - 1) as usize
        };
        if start_index >= chars.len() {
            return String::new();
        }

        let end_index = match length {
            Some(length) => {
                let len = self.eval_numeric_expression(length).unwrap_or(0.0) as i64;
                if len <= 0 {
                    return String::new();
                }
                (start_index + len as usize).min(chars.len())
            }
            None => chars.len(),
        };

        chars[start_index..end_index].iter().collect()
    }

    fn eval_rand(&self) -> f64 {
        // Deterministic LCG so tests remain stable.
        let next = self
            .rng_state
            .get()
            .wrapping_mul(1_103_515_245)
            .wrapping_add(12_345)
            & 0x7fff_ffff;
        self.rng_state.set(next);
        (next as f64) / 2_147_483_648.0
    }

    fn eval_function_call(&mut self, name: &str, args: &[Expression<'_>]) -> String {
        match name {
            "sprintf" => {
                if args.is_empty() {
                    return String::new();
                }
                let format = unescape_awk_string(&self.eval_expression(&args[0]));
                let values: Vec<String> = args
                    .iter()
                    .skip(1)
                    .map(|arg| self.eval_printf_argument(arg))
                    .collect();
                format_printf(&format, &values)
            }
            "split" => {
                let count = match (args.first(), args.get(1), args.get(2)) {
                    (Some(string), Some(Expression::Identifier(array)), separator) => {
                        self.eval_split(string, array, separator)
                    }
                    _ => 0,
                };
                format_awk_number(count as f64)
            }
            "index" => {
                let string = args
                    .first()
                    .map(|arg| self.eval_expression(arg))
                    .unwrap_or_default();
                let search = args
                    .get(1)
                    .map(|arg| self.eval_expression(arg))
                    .unwrap_or_default();
                match string.find(&search) {
                    Some(index) => format_awk_number((index + 1) as f64),
                    None => "0".to_string(),
                }
            }
            "match" => format_awk_number(self.eval_match_function(args)),
            "sqrt" => {
                let value = args
                    .first()
                    .and_then(|arg| self.eval_numeric_expression(arg))
                    .unwrap_or(0.0);
                if value < 0.0 {
                    self.runtime_error =
                        Some(format!("sqrt called with negative argument {value}"));
                    return String::new();
                }
                format_awk_number(value.sqrt())
            }
            "log" => {
                let value = args
                    .first()
                    .and_then(|arg| self.eval_numeric_expression(arg))
                    .unwrap_or(0.0);
                if value <= 0.0 {
                    self.runtime_error =
                        Some(format!("log called with non-positive argument {value}"));
                    return String::new();
                }
                format_awk_number(value.ln())
            }
            "exp" => {
                let value = args
                    .first()
                    .and_then(|arg| self.eval_numeric_expression(arg))
                    .unwrap_or(0.0);
                format_awk_number(value.exp())
            }
            "sin" => {
                let value = args
                    .first()
                    .and_then(|arg| self.eval_numeric_expression(arg))
                    .unwrap_or(0.0);
                format_awk_number(value.sin())
            }
            "cos" => {
                let value = args
                    .first()
                    .and_then(|arg| self.eval_numeric_expression(arg))
                    .unwrap_or(0.0);
                format_awk_number(value.cos())
            }
            "int" => {
                let value = args
                    .first()
                    .and_then(|arg| self.eval_numeric_expression(arg))
                    .unwrap_or(0.0);
                format_awk_number(value.trunc())
            }
            "srand" => {
                let seed = args
                    .first()
                    .and_then(|arg| self.eval_numeric_expression(arg))
                    .unwrap_or(1.0) as u64;
                self.rng_state.set(seed);
                format_awk_number(seed as f64)
            }
            _ if self.program.function_definition(name).is_some() => {
                self.eval_user_defined_function_call(name, args).value
            }
            _ => "0".to_string(),
        }
    }

    fn eval_numeric_function_call(&mut self, name: &str, args: &[Expression<'_>]) -> Option<f64> {
        match name {
            "split" => match (args.first(), args.get(1), args.get(2)) {
                (Some(string), Some(Expression::Identifier(array)), separator) => {
                    Some(self.eval_split(string, array, separator) as f64)
                }
                _ => Some(0.0),
            },
            "index" => {
                let string = args
                    .first()
                    .map(|arg| self.eval_expression(arg))
                    .unwrap_or_default();
                let search = args
                    .get(1)
                    .map(|arg| self.eval_expression(arg))
                    .unwrap_or_default();
                Some(
                    string
                        .find(&search)
                        .map(|index| (index + 1) as f64)
                        .unwrap_or(0.0),
                )
            }
            "match" => Some(self.eval_match_function(args)),
            "sqrt" => {
                let value = args
                    .first()
                    .and_then(|arg| self.eval_numeric_expression(arg))
                    .unwrap_or(0.0);
                if value < 0.0 {
                    self.runtime_error =
                        Some(format!("sqrt called with negative argument {value}"));
                    return None;
                }
                Some(value.sqrt())
            }
            "log" => {
                let value = args
                    .first()
                    .and_then(|arg| self.eval_numeric_expression(arg))
                    .unwrap_or(0.0);
                if value <= 0.0 {
                    self.runtime_error =
                        Some(format!("log called with non-positive argument {value}"));
                    return None;
                }
                Some(value.ln())
            }
            "exp" => args
                .first()
                .and_then(|arg| self.eval_numeric_expression(arg))
                .map(f64::exp),
            "sin" => args
                .first()
                .and_then(|arg| self.eval_numeric_expression(arg))
                .map(f64::sin),
            "cos" => args
                .first()
                .and_then(|arg| self.eval_numeric_expression(arg))
                .map(f64::cos),
            "int" => args
                .first()
                .and_then(|arg| self.eval_numeric_expression(arg))
                .map(f64::trunc),
            "srand" => {
                let seed = args
                    .first()
                    .and_then(|arg| self.eval_numeric_expression(arg))
                    .unwrap_or(1.0) as u64;
                self.rng_state.set(seed);
                Some(seed as f64)
            }
            _ => None,
        }
    }

    fn eval_match_function(&mut self, args: &[Expression<'_>]) -> f64 {
        let text = args
            .first()
            .map(|arg| self.eval_expression(arg))
            .unwrap_or_default();
        let matched = match args.get(1) {
            Some(Expression::Regex(pattern)) => find_awk_regex_match(&text, pattern),
            Some(other) => find_awk_regex_match(&text, &self.eval_expression(other)),
            None => None,
        };

        match matched {
            Some((start, length)) => {
                self.set_variable_numeric("RSTART", (start + 1) as f64);
                self.set_variable_numeric("RLENGTH", length as f64);
                (start + 1) as f64
            }
            None => {
                self.set_variable_numeric("RSTART", 0.0);
                self.set_variable_numeric("RLENGTH", -1.0);
                0.0
            }
        }
    }

    fn eval_user_defined_function_call(
        &mut self,
        name: &str,
        args: &[Expression<'_>],
    ) -> FunctionCallResult {
        let Some(definition) = self.program.function_definition(name).cloned() else {
            return FunctionCallResult {
                value: self.eval_function_call(name, args),
                output: Vec::new(),
            };
        };

        let argument_values: Vec<String> =
            args.iter().map(|arg| self.eval_expression(arg)).collect();

        let mut saved_values = Vec::new();
        let mut saved_array_aliases = Vec::new();
        for parameter in &definition.parameters {
            saved_values.push((
                *parameter,
                self.variables.get(*parameter).cloned(),
                self.numeric_variables.get(*parameter).copied(),
            ));
            saved_array_aliases.push((*parameter, self.array_aliases.get(*parameter).cloned()));
        }

        for (index, parameter) in definition.parameters.iter().enumerate() {
            let value = argument_values.get(index).cloned().unwrap_or_default();
            self.set_variable_text(parameter, value);
            if let Some(Expression::Identifier(identifier)) = args.get(index) {
                self.array_aliases
                    .insert((*parameter).to_string(), (*identifier).to_string());
            } else {
                self.array_aliases.remove(*parameter);
            }
        }

        let saved_return_value = self.return_value.take();
        let mut output = Vec::new();
        for statement in &definition.statements {
            let statement_output = self.eval_statement(statement, None);
            self.append_local_output(&mut output, statement_output);
            if self.should_break_function_body() {
                break;
            }
        }

        let return_value = self.return_value.take().unwrap_or_default();
        self.return_value = saved_return_value;

        for (parameter, prior_value, prior_numeric_value) in saved_values {
            if let Some(value) = prior_value {
                self.variables.insert(parameter.to_string(), value);
                if let Some(numeric) = prior_numeric_value {
                    self.numeric_variables
                        .insert(parameter.to_string(), numeric);
                } else {
                    self.numeric_variables.remove(parameter);
                }
            } else {
                self.variables.remove(parameter);
                self.numeric_variables.remove(parameter);
            }
        }
        for (parameter, prior_alias) in saved_array_aliases {
            if let Some(alias) = prior_alias {
                self.array_aliases.insert(parameter.to_string(), alias);
            } else {
                self.array_aliases.remove(parameter);
            }
        }

        FunctionCallResult {
            value: return_value,
            output,
        }
    }

    fn split_fields(&self, line: &str) -> Vec<String> {
        if line.is_empty() {
            return Vec::new();
        }

        if self.field_separator == " " {
            line.split_whitespace().map(str::to_string).collect()
        } else {
            split_with_regex(line, &self.field_separator)
        }
    }

    fn split_source(&mut self, source: &str, separator: Option<&Expression<'_>>) -> Vec<String> {
        match separator {
            None => self.split_fields(source),
            Some(Expression::Regex(pattern)) => split_with_regex(source, pattern),
            Some(expression) => {
                let separator = self.eval_expression(expression);
                if separator == " " {
                    source.split_whitespace().map(str::to_string).collect()
                } else {
                    split_with_regex(source, &separator)
                }
            }
        }
    }

    fn split_line_into_fields(&self, line: &str) -> Vec<String> {
        if self.variables.contains_key("NF")
            && !self.output_field_separator.is_empty()
            && line.contains(&self.output_field_separator)
        {
            return line
                .split(&self.output_field_separator)
                .map(str::to_string)
                .collect();
        }
        self.split_fields(line)
    }

    fn eval_numeric_infix(
        &mut self,
        left: &Expression<'_>,
        operator: &crate::token::Token<'_>,
        right: &Expression<'_>,
    ) -> Option<f64> {
        if matches!(
            operator.kind,
            TokenKind::Assign
                | TokenKind::AddAssign
                | TokenKind::SubtractAssign
                | TokenKind::MultiplyAssign
                | TokenKind::DivideAssign
                | TokenKind::ModuloAssign
                | TokenKind::PowerAssign
        ) {
            let identifier = match left {
                Expression::Identifier(identifier) => *identifier,
                _ => return None,
            };
            let right_value = self.eval_numeric_expression(right).unwrap_or(0.0);
            let current = parse_awk_numeric(&self.eval_identifier_expression(identifier));
            if matches!(operator.kind, TokenKind::DivideAssign | TokenKind::ModuloAssign)
                && right_value == 0.0
            {
                self.runtime_error = Some("division by zero".to_string());
                return None;
            }
            let updated = match operator.kind {
                TokenKind::Assign => right_value,
                TokenKind::AddAssign => current + right_value,
                TokenKind::SubtractAssign => current - right_value,
                TokenKind::MultiplyAssign => current * right_value,
                TokenKind::DivideAssign => current / right_value,
                TokenKind::ModuloAssign => current % right_value,
                TokenKind::PowerAssign => current.powf(right_value),
                _ => unreachable!(),
            };
            self.set_variable_numeric(identifier, updated);
            return Some(updated);
        }

        let left_value = self.eval_numeric_expression(left)?;
        let right_value = self.eval_numeric_expression(right)?;

        match operator.kind {
            TokenKind::Plus => Some(left_value + right_value),
            TokenKind::Minus => Some(left_value - right_value),
            TokenKind::Asterisk => Some(left_value * right_value),
            TokenKind::Division => {
                if right_value == 0.0 {
                    self.runtime_error = Some("division by zero".to_string());
                    return None;
                }
                Some(left_value / right_value)
            }
            TokenKind::Percent => {
                if right_value == 0.0 {
                    self.runtime_error = Some("division by zero".to_string());
                    return None;
                }
                Some(left_value % right_value)
            }
            TokenKind::Caret => Some(left_value.powf(right_value)),
            TokenKind::GreaterThan => Some(if left_value > right_value { 1.0 } else { 0.0 }),
            TokenKind::GreaterThanOrEqual => {
                Some(if left_value >= right_value { 1.0 } else { 0.0 })
            }
            TokenKind::LessThan => Some(if left_value < right_value { 1.0 } else { 0.0 }),
            TokenKind::LessThanOrEqual => Some(if left_value <= right_value { 1.0 } else { 0.0 }),
            TokenKind::Equal => Some(if left_value == right_value { 1.0 } else { 0.0 }),
            TokenKind::NotEqual => Some(if left_value != right_value { 1.0 } else { 0.0 }),
            _ => None,
        }
    }

    fn eval_numeric_expression(&mut self, expression: &Expression<'_>) -> Option<f64> {
        match expression {
            Expression::Number(value) => Some(*value),
            Expression::HexNumber { value, .. } => Some(*value),
            Expression::Identifier(identifier) => Some(
                self.numeric_variables
                    .get(*identifier)
                    .copied()
                    .unwrap_or_else(|| {
                        parse_awk_numeric(&self.eval_identifier_expression(identifier))
                    }),
            ),
            Expression::ArrayAccess { identifier, index } => Some(parse_awk_numeric(
                &self.eval_array_access(identifier, index),
            )),
            Expression::Field(inner) => Some(parse_awk_numeric(&self.eval_field_expression(inner))),
            Expression::Length(expression) => self
                .eval_length_expression(expression.as_deref())
                .parse::<f64>()
                .ok(),
            Expression::Rand => Some(self.eval_rand()),
            Expression::FunctionCall { name, args } => self
                .eval_numeric_function_call(name, args)
                .or_else(|| self.eval_function_call(name, args).parse().ok()),
            Expression::Not(expression) => Some(if self.eval_condition(expression) {
                0.0
            } else {
                1.0
            }),
            Expression::PreIncrement(_)
            | Expression::PreDecrement(_)
            | Expression::PostIncrement(_)
            | Expression::PostDecrement(_) => self.eval_expression(expression).parse::<f64>().ok(),
            Expression::Ternary {
                condition,
                then_expr,
                else_expr,
            } => {
                if self.eval_condition(condition) {
                    self.eval_numeric_expression(then_expr)
                } else {
                    self.eval_numeric_expression(else_expr)
                }
            }
            Expression::Concatenation { left, right } => {
                let mut value = self.eval_expression(left);
                value.push_str(&self.eval_expression(right));
                value.parse::<f64>().ok()
            }
            Expression::Infix {
                left,
                operator,
                right,
            } => self.eval_numeric_infix(left, operator, right),
            _ => None,
        }
    }

    fn eval_condition(&mut self, expression: &Expression<'_>) -> bool {
        if let Expression::Not(inner) = expression {
            return !self.eval_condition(inner);
        }

        if let Expression::Regex(pattern) = expression {
            return self
                .current_line
                .as_deref()
                .is_some_and(|line| awk_regex_matches(line, pattern));
        }

        if let Expression::Infix {
            left,
            operator,
            right,
        } = expression
        {
            if let Some(value) = self.eval_logical(left, operator.kind.clone(), right) {
                return value;
            }
            if let Some(value) = self.eval_regex_match(left, operator.kind.clone(), right) {
                return value;
            }
            if let Some(value) = self.eval_membership(left, operator.kind.clone(), right) {
                return value;
            }
            if let Some(value) = self.eval_comparison(left, operator.kind.clone(), right) {
                return value;
            }
        }

        let value = self.eval_expression(expression);
        awk_truthy(&value)
    }

    fn eval_comparison(
        &mut self,
        left: &Expression<'_>,
        operator: TokenKind,
        right: &Expression<'_>,
    ) -> Option<bool> {
        if !matches!(
            operator,
            TokenKind::Equal
                | TokenKind::NotEqual
                | TokenKind::GreaterThan
                | TokenKind::GreaterThanOrEqual
                | TokenKind::LessThan
                | TokenKind::LessThanOrEqual
        ) {
            return None;
        }

        let left_value = self.comparison_operand(left);
        let right_value = self.comparison_operand(right);

        let result = match (left_value.numeric, right_value.numeric) {
            (Some(l), Some(r)) => match operator {
                TokenKind::Equal => l == r,
                TokenKind::NotEqual => l != r,
                TokenKind::GreaterThan => l > r,
                TokenKind::GreaterThanOrEqual => l >= r,
                TokenKind::LessThan => l < r,
                TokenKind::LessThanOrEqual => l <= r,
                _ => unreachable!(),
            },
            _ => match operator {
                TokenKind::Equal => left_value.text == right_value.text,
                TokenKind::NotEqual => left_value.text != right_value.text,
                TokenKind::GreaterThan => left_value.text > right_value.text,
                TokenKind::GreaterThanOrEqual => left_value.text >= right_value.text,
                TokenKind::LessThan => left_value.text < right_value.text,
                TokenKind::LessThanOrEqual => left_value.text <= right_value.text,
                _ => unreachable!(),
            },
        };

        Some(result)
    }

    fn comparison_operand(&mut self, expression: &Expression<'_>) -> ComparisonOperand {
        match expression {
            Expression::Identifier(identifier) => {
                let text = self.eval_identifier_expression(identifier);
                let numeric = self
                    .numeric_variables
                    .get(*identifier)
                    .copied()
                    .or_else(|| parse_full_awk_numeric(&text))
                    .or_else(|| {
                        if !self.variables.contains_key(*identifier)
                            && !is_special_identifier(identifier)
                        {
                            Some(0.0)
                        } else {
                            None
                        }
                    });
                ComparisonOperand { text, numeric }
            }
            Expression::Number(_)
            | Expression::HexNumber { .. }
            | Expression::Infix { .. }
            | Expression::Length(_)
            | Expression::Rand => {
                let text = self.eval_expression(expression);
                let numeric =
                    parse_full_awk_numeric(&text).or_else(|| Some(parse_awk_numeric(&text)));
                ComparisonOperand { text, numeric }
            }
            Expression::String(_) => {
                let text = self.eval_expression(expression);
                ComparisonOperand {
                    text,
                    numeric: None,
                }
            }
            Expression::Field(_)
            | Expression::ArrayAccess { .. }
            | Expression::Substr { .. }
            | Expression::FunctionCall { .. }
            | Expression::Regex(_)
            | Expression::Concatenation { .. }
            | Expression::Not(_)
            | Expression::PreIncrement(_)
            | Expression::PreDecrement(_)
            | Expression::PostIncrement(_)
            | Expression::PostDecrement(_)
            | Expression::Ternary { .. } => {
                let text = self.eval_expression(expression);
                let numeric = if text.is_empty() {
                    None
                } else {
                    parse_full_awk_numeric(&text)
                };
                ComparisonOperand { text, numeric }
            }
        }
    }

    fn eval_regex_match(
        &mut self,
        left: &Expression<'_>,
        operator: TokenKind,
        right: &Expression<'_>,
    ) -> Option<bool> {
        if !matches!(operator, TokenKind::Tilde | TokenKind::NoMatch) {
            return None;
        }

        let haystack = self.eval_expression(left);
        let matches = match right {
            Expression::Regex(pattern) => awk_regex_matches(&haystack, pattern),
            _ => awk_regex_matches(&haystack, &self.eval_expression(right)),
        };
        Some(if operator == TokenKind::NoMatch {
            !matches
        } else {
            matches
        })
    }

    fn eval_logical(
        &mut self,
        left: &Expression<'_>,
        operator: TokenKind,
        right: &Expression<'_>,
    ) -> Option<bool> {
        match operator {
            TokenKind::And => {
                if !self.eval_condition(left) {
                    Some(false)
                } else {
                    Some(self.eval_condition(right))
                }
            }
            TokenKind::Or => {
                if self.eval_condition(left) {
                    Some(true)
                } else {
                    Some(self.eval_condition(right))
                }
            }
            _ => None,
        }
    }

    fn eval_membership(
        &mut self,
        left: &Expression<'_>,
        operator: TokenKind,
        right: &Expression<'_>,
    ) -> Option<bool> {
        if operator != TokenKind::In {
            return None;
        }

        let identifier = match right {
            Expression::Identifier(identifier) => *identifier,
            _ => return Some(false),
        };
        let key = self.array_key(identifier, left);
        Some(self.array_variables.contains_key(&key))
    }
}

fn awk_regex_matches(text: &str, pattern: &str) -> bool {
    if let Ok(re) = Regex::new(pattern) {
        return re.is_match(text);
    }

    awk_regex_matches_legacy(text, pattern)
}

fn find_awk_regex_match(text: &str, pattern: &str) -> Option<(usize, usize)> {
    if let Ok(re) = Regex::new(pattern) {
        return re.find(text).map(|matched| {
            let start = text[..matched.start()].chars().count();
            let length = text[matched.start()..matched.end()].chars().count();
            (start, length)
        });
    }

    find_awk_regex_match_legacy(text, pattern)
}

fn awk_regex_matches_legacy(text: &str, pattern: &str) -> bool {
    let anchored_start = pattern.starts_with('^');
    let anchored_end = pattern.ends_with('$');
    let mut core = pattern;

    if anchored_start {
        core = &core[1..];
    }
    if anchored_end && !core.is_empty() {
        core = &core[..core.len() - 1];
    }

    if core == "[0-9]+" {
        return match (anchored_start, anchored_end) {
            (true, true) => !text.is_empty() && text.chars().all(|c| c.is_ascii_digit()),
            (true, false) => text.chars().next().is_some_and(|c| c.is_ascii_digit()),
            (false, true) => text.chars().last().is_some_and(|c| c.is_ascii_digit()),
            (false, false) => text.chars().any(|c| c.is_ascii_digit()),
        };
    }

    if core == "." {
        return match (anchored_start, anchored_end) {
            (true, true) => text.chars().count() == 1,
            (true, false) => !text.is_empty(),
            (false, true) => !text.is_empty(),
            (false, false) => !text.is_empty(),
        };
    }

    if core.starts_with('[') && core.ends_with(']') && core.len() >= 3 {
        let class = &core[1..core.len() - 1];
        let class_matches = |ch: char| -> bool {
            let mut chars = class.chars().peekable();
            while let Some(start) = chars.next() {
                if chars.peek() == Some(&'-') {
                    chars.next();
                    if let Some(end) = chars.next() {
                        if start <= ch && ch <= end {
                            return true;
                        }
                        continue;
                    }
                    if start == ch || '-' == ch {
                        return true;
                    }
                    break;
                }
                if start == ch {
                    return true;
                }
            }
            false
        };

        return match (anchored_start, anchored_end) {
            (true, true) => {
                text.chars().count() == 1 && text.chars().next().is_some_and(class_matches)
            }
            (true, false) => text.chars().next().is_some_and(class_matches),
            (false, true) => text.chars().last().is_some_and(class_matches),
            (false, false) => text.chars().any(class_matches),
        };
    }

    if core.starts_with('(') && core.ends_with(')') && core.contains('|') {
        return core[1..core.len() - 1].split('|').any(|alt| {
            match (anchored_start, anchored_end) {
                (true, true) => text == alt,
                (true, false) => text.starts_with(alt),
                (false, true) => text.ends_with(alt),
                (false, false) => text.contains(alt),
            }
        });
    }

    if core.contains('|') {
        return core
            .split('|')
            .any(|alt| match (anchored_start, anchored_end) {
                (true, true) => text == alt,
                (true, false) => text.starts_with(alt),
                (false, true) => text.ends_with(alt),
                (false, false) => text.contains(alt),
            });
    }

    if anchored_start && anchored_end {
        return text == core;
    }
    if anchored_start {
        return text.starts_with(core);
    }
    if anchored_end {
        return text.ends_with(core);
    }
    text.contains(core)
}

fn find_awk_regex_match_legacy(text: &str, pattern: &str) -> Option<(usize, usize)> {
    let anchored_start = pattern.starts_with('^');
    let anchored_end = pattern.ends_with('$');
    let mut core = pattern;

    if anchored_start {
        core = &core[1..];
    }
    if anchored_end && !core.is_empty() {
        core = &core[..core.len() - 1];
    }

    if core == "[0-9]+" {
        let chars: Vec<char> = text.chars().collect();
        for start in 0..chars.len() {
            if anchored_start && start != 0 {
                break;
            }
            if !chars[start].is_ascii_digit() {
                continue;
            }
            let mut end = start;
            while end < chars.len() && chars[end].is_ascii_digit() {
                end += 1;
            }
            if anchored_end && end != chars.len() {
                continue;
            }
            return Some((start, end - start));
        }
        return None;
    }

    if core == "." {
        return if text.is_empty() {
            None
        } else if anchored_end {
            Some((text.chars().count() - 1, 1))
        } else {
            Some((0, 1))
        };
    }

    if core.starts_with('(') && core.ends_with(')') && core.contains('|') {
        for alt in core[1..core.len() - 1].split('|') {
            if let Some(found) = find_awk_regex_match_legacy(text, alt) {
                return Some(found);
            }
        }
        return None;
    }

    if core.contains('|') {
        for alt in core.split('|') {
            if let Some(found) = find_awk_regex_match_legacy(text, alt) {
                return Some(found);
            }
        }
        return None;
    }

    if core.starts_with('[') && core.ends_with(']') && core.len() >= 3 {
        let class = &core[1..core.len() - 1];
        let class_matches = |ch: char| -> bool {
            let mut chars = class.chars().peekable();
            while let Some(start) = chars.next() {
                if chars.peek() == Some(&'-') {
                    chars.next();
                    if let Some(end) = chars.next() {
                        if start <= ch && ch <= end {
                            return true;
                        }
                        continue;
                    }
                    if start == ch || '-' == ch {
                        return true;
                    }
                    break;
                }
                if start == ch {
                    return true;
                }
            }
            false
        };

        let chars: Vec<char> = text.chars().collect();
        for (idx, ch) in chars.iter().enumerate() {
            if anchored_start && idx != 0 {
                break;
            }
            if !class_matches(*ch) {
                continue;
            }
            if anchored_end && idx + 1 != chars.len() {
                continue;
            }
            return Some((idx, 1));
        }
        return None;
    }

    if core.is_empty() {
        return Some((0, 0));
    }

    if anchored_start && anchored_end {
        return (text == core).then_some((0, core.chars().count()));
    }
    if anchored_start {
        return text.starts_with(core).then_some((0, core.chars().count()));
    }
    if anchored_end {
        return text.ends_with(core).then_some((
            text.chars().count().saturating_sub(core.chars().count()),
            core.chars().count(),
        ));
    }

    let byte_start = text.find(core)?;
    Some((text[..byte_start].chars().count(), core.chars().count()))
}

fn awk_gsub_replace_all(text: &str, pattern: &str, replacement: &str) -> String {
    if let Ok(re) = Regex::new(pattern) {
        let mut out = String::new();
        let mut last = 0usize;
        for m in re.find_iter(text) {
            out.push_str(&text[last..m.start()]);
            out.push_str(&awk_subst_replacement(
                replacement,
                &text[m.start()..m.end()],
            ));
            last = m.end();
            if m.start() == m.end() {
                if let Some((next_idx, ch)) = text[last..].char_indices().next() {
                    let char_end = last + next_idx + ch.len_utf8();
                    out.push_str(&text[last..char_end]);
                    last = char_end;
                } else {
                    break;
                }
            }
        }
        out.push_str(&text[last..]);
        return out;
    }

    let anchored_start = pattern.starts_with('^');
    let anchored_end = pattern.ends_with('$');
    let mut core = pattern;

    if anchored_start {
        core = &core[1..];
    }
    if anchored_end && !core.is_empty() {
        core = &core[..core.len() - 1];
    }
    if core.is_empty() {
        return text.to_string();
    }

    match (anchored_start, anchored_end) {
        (true, true) => {
            if text == core {
                awk_subst_replacement(replacement, core)
            } else {
                text.to_string()
            }
        }
        (true, false) => {
            if let Some(suffix) = text.strip_prefix(core) {
                format!("{}{}", awk_subst_replacement(replacement, core), suffix)
            } else {
                text.to_string()
            }
        }
        (false, true) => {
            if let Some(prefix) = text.strip_suffix(core) {
                format!("{prefix}{}", awk_subst_replacement(replacement, core))
            } else {
                text.to_string()
            }
        }
        (false, false) => {
            let mut out = String::new();
            let mut last = 0usize;
            while let Some(relative_pos) = text[last..].find(core) {
                let start = last + relative_pos;
                out.push_str(&text[last..start]);
                out.push_str(&awk_subst_replacement(replacement, core));
                last = start + core.len();
            }
            out.push_str(&text[last..]);
            out
        }
    }
}

fn awk_sub_replace_first(text: &str, pattern: &str, replacement: &str) -> String {
    if let Ok(re) = Regex::new(pattern)
        && let Some(m) = re.find(text)
    {
        let mut out = String::new();
        out.push_str(&text[..m.start()]);
        out.push_str(&awk_subst_replacement(
            replacement,
            &text[m.start()..m.end()],
        ));
        out.push_str(&text[m.end()..]);
        return out;
    }

    let replaced = awk_gsub_replace_all(text, pattern, replacement);
    if replaced == text {
        return text.to_string();
    }

    // Legacy fallback replaces all; emulate single replacement by finding first change boundary.
    // Keep this simple for non-regex fallback patterns used by existing tests.
    if let Some(pos) = text.find(pattern) {
        let mut out = String::new();
        out.push_str(&text[..pos]);
        out.push_str(replacement);
        out.push_str(&text[pos + pattern.len()..]);
        return out;
    }

    replaced
}

fn awk_subst_replacement(replacement: &str, matched: &str) -> String {
    let mut out = String::new();
    let mut chars = replacement.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.peek().copied() {
                Some('&') => {
                    chars.next();
                    out.push('&');
                }
                Some('\\') => {
                    chars.next();
                    out.push('\\');
                }
                Some(next) => {
                    out.push('\\');
                    out.push(next);
                    chars.next();
                }
                None => out.push('\\'),
            }
            continue;
        }
        if ch == '&' {
            out.push_str(matched);
        } else {
            out.push(ch);
        }
    }
    out
}

fn unescape_awk_string(input: &str) -> String {
    let mut output = String::new();
    let mut chars = input.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch != '\\' {
            output.push(ch);
            continue;
        }

        match chars.next() {
            Some('n') => output.push('\n'),
            Some('t') => output.push('\t'),
            Some('r') => output.push('\r'),
            Some('\\') => output.push('\\'),
            Some('"') => output.push('"'),
            Some(other) => {
                output.push('\\');
                output.push(other);
            }
            None => output.push('\\'),
        }
    }

    output
}

fn normalize_output_lines(lines: Vec<String>) -> Vec<String> {
    let output = lines.concat();
    if output.is_empty() {
        return if lines.is_empty() {
            Vec::new()
        } else {
            vec![String::new()]
        };
    }

    output.lines().map(str::to_string).collect()
}

fn format_printf(format: &str, args: &[String]) -> String {
    let mut result = String::new();
    let mut chars = format.chars().peekable();
    let mut arg_index = 0usize;

    while let Some(ch) = chars.next() {
        if ch != '%' {
            result.push(ch);
            continue;
        }

        if chars.peek() == Some(&'%') {
            chars.next();
            result.push('%');
            continue;
        }

        let mut left_justify = false;
        if chars.peek() == Some(&'-') {
            left_justify = true;
            chars.next();
        }

        let mut width: usize = 0;
        while let Some(next) = chars.peek() {
            if next.is_ascii_digit() {
                width = (width * 10) + (*next as usize - '0' as usize);
                chars.next();
            } else {
                break;
            }
        }

        let mut precision: Option<usize> = None;
        if chars.peek() == Some(&'.') {
            chars.next();
            let mut value = 0usize;
            while let Some(next) = chars.peek() {
                if next.is_ascii_digit() {
                    value = (value * 10) + (*next as usize - '0' as usize);
                    chars.next();
                } else {
                    break;
                }
            }
            precision = Some(value);
        }

        let specifier = match chars.next() {
            Some(value) => value,
            None => {
                result.push('%');
                break;
            }
        };

        let arg = args.get(arg_index).cloned().unwrap_or_default();
        arg_index += 1;

        let formatted = match specifier {
            's' => {
                if let Some(precision) = precision {
                    arg.chars().take(precision).collect()
                } else {
                    arg
                }
            }
            'd' => (parse_awk_numeric(&arg).trunc() as i64).to_string(),
            'u' => (parse_awk_numeric(&arg).trunc() as i64 as u64).to_string(),
            'o' => format!("{:o}", parse_awk_numeric(&arg).trunc() as i64 as u64),
            'x' => format!("{:x}", parse_awk_numeric(&arg).trunc() as i64 as u64),
            'X' => format!("{:X}", parse_awk_numeric(&arg).trunc() as i64 as u64),
            'c' => arg.chars().next().unwrap_or('\0').to_string(),
            'f' => {
                let value = parse_awk_numeric(&arg);
                let precision = precision.unwrap_or(6);
                format!("{value:.precision$}")
            }
            _ => {
                result.push('%');
                if left_justify {
                    result.push('-');
                }
                if width > 0 {
                    result.push_str(&width.to_string());
                }
                if let Some(precision) = precision {
                    result.push('.');
                    result.push_str(&precision.to_string());
                }
                result.push(specifier);
                continue;
            }
        };

        if width <= formatted.len() {
            result.push_str(&formatted);
            continue;
        }

        let padding_len = width - formatted.len();
        let padding = " ".repeat(padding_len);
        if left_justify {
            result.push_str(&formatted);
            result.push_str(&padding);
        } else {
            result.push_str(&padding);
            result.push_str(&formatted);
        }
    }

    result
}

fn parse_awk_numeric(input: &str) -> f64 {
    let s = input.trim_start();
    if s.is_empty() {
        return 0.0;
    }

    let mut end = 0usize;
    let mut seen_digit = false;
    let mut seen_dot = false;

    for (idx, ch) in s.char_indices() {
        if ch.is_ascii_digit() {
            seen_digit = true;
            end = idx + ch.len_utf8();
            continue;
        }
        if (ch == '+' || ch == '-') && idx == 0 {
            end = idx + ch.len_utf8();
            continue;
        }
        if ch == '.' && !seen_dot {
            seen_dot = true;
            end = idx + ch.len_utf8();
            continue;
        }
        break;
    }

    if !seen_digit {
        return 0.0;
    }

    s[..end].parse::<f64>().unwrap_or(0.0)
}

fn parse_full_awk_numeric(input: &str) -> Option<f64> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }

    s.parse::<f64>().ok()
}

fn expression_has_precise_numeric_value(expression: &Expression<'_>) -> bool {
    match expression {
        Expression::Number(_)
        | Expression::HexNumber { .. }
        | Expression::Length(_)
        | Expression::Rand => true,
        Expression::Field(_) => false,
        Expression::Identifier(_) | Expression::ArrayAccess { .. } => false,
        Expression::FunctionCall { name, .. } => matches!(
            *name,
            "index"
                | "length"
                | "split"
                | "sqrt"
                | "log"
                | "exp"
                | "sin"
                | "cos"
                | "int"
                | "srand"
                | "rand"
        ),
        Expression::Not(_)
        | Expression::PreIncrement(_)
        | Expression::PreDecrement(_)
        | Expression::PostIncrement(_)
        | Expression::PostDecrement(_) => false,
        Expression::Ternary {
            then_expr,
            else_expr,
            ..
        } => {
            expression_has_precise_numeric_value(then_expr)
                && expression_has_precise_numeric_value(else_expr)
        }
        Expression::Concatenation { .. } | Expression::String(_) | Expression::Regex(_) => false,
        Expression::Substr { .. } => false,
        Expression::Infix { operator, .. } => matches!(
            operator.kind,
            TokenKind::Plus
                | TokenKind::Minus
                | TokenKind::Asterisk
                | TokenKind::Division
                | TokenKind::Percent
                | TokenKind::Caret
                | TokenKind::GreaterThan
                | TokenKind::GreaterThanOrEqual
                | TokenKind::LessThan
                | TokenKind::LessThanOrEqual
                | TokenKind::Equal
                | TokenKind::NotEqual
        ),
    }
}

fn is_special_identifier(identifier: &str) -> bool {
    matches!(
        identifier,
        "getline" | "FS" | "OFS" | "ORS" | "NF" | "NR" | "FNR" | "FILENAME" | "ARGC"
    )
}

fn awk_truthy(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }

    parse_full_awk_numeric(value) != Some(0.0)
}

fn split_with_regex(source: &str, pattern: &str) -> Vec<String> {
    if source.is_empty() {
        return Vec::new();
    }

    let Ok(regex) = Regex::new(pattern) else {
        return source.split(pattern).map(str::to_string).collect();
    };

    let mut fields = Vec::new();
    let mut last_end = 0;
    for matched in regex.find_iter(source) {
        fields.push(source[last_end..matched.start()].to_string());
        last_end = matched.end();
    }
    fields.push(source[last_end..].to_string());
    fields
}

fn format_awk_number(value: f64) -> String {
    if !value.is_finite() {
        return value.to_string();
    }

    if value == 0.0 {
        return "0".to_string();
    }

    if value.fract() == 0.0 {
        return format!("{value:.0}");
    }

    let abs = value.abs();
    let exponent = abs.log10().floor() as i32;
    let decimals = (6 - exponent - 1).max(0) as usize;
    let formatted = format!("{value:.decimals$}");
    formatted
        .trim_end_matches('0')
        .trim_end_matches('.')
        .to_string()
}

#[cfg(test)]
mod tests {
    use crate::{Lexer, Parser};

    use super::*;

    #[test]
    fn eval_rule_for_line_returns_empty_when_pattern_does_not_match() {
        let mut evaluator = Evaluator::new(Program::new(), vec![], "-");
        let rule = Rule::PatternAction {
            pattern: Some(Expression::Number(0.0)),
            action: Some(Action {
                statements: vec![Statement::Print(vec![Expression::String("unexpected")])],
            }),
        };
        let mut range_active = false;

        let output = evaluator.eval_rule_for_line(&rule, "input", &mut range_active);

        assert!(output.is_empty());
        assert!(!range_active);
    }

    #[test]
    fn eval_rule_for_line_ignores_begin_rules_during_record_processing() {
        let mut evaluator = Evaluator::new(Program::new(), vec![], "-");
        let rule = Rule::Begin(Action {
            statements: vec![Statement::Print(vec![Expression::String("unexpected")])],
        });
        let mut range_active = false;

        let output = evaluator.eval_rule_for_line(&rule, "input", &mut range_active);

        assert!(output.is_empty());
        assert!(!range_active);
    }

    #[test]
    fn eval_print_action_outputs_input_line() {
        let lexer = Lexer::new("{ print }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["hello, world!".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output.len(), 1);
        assert_eq!(output[0], "hello, world!");
    }

    #[test]
    fn eval_begin_print_string_literal() {
        let lexer = Lexer::new(r#"BEGIN { print "hello" }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["hello".to_string()]);
    }

    #[test]
    fn eval_end_print_string_literal() {
        let lexer = Lexer::new(r#"END { print "42" } { print "hello" }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["one row".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["hello".to_string(), "42".to_string()]);
    }

    #[test]
    fn eval_print_numeric_plus_infix_expression() {
        let lexer = Lexer::new(r#"BEGIN { print 1 + 2 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["3".to_string()]);
    }

    #[test]
    fn eval_print_numberic_multiply_infix_expression() {
        let lexer = Lexer::new(r#"BEGIN { print 2 * 3 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["6".to_string()]);
    }

    #[test]
    fn eval_print_numeric_mod_infix_expression() {
        let lexer = Lexer::new(r#"BEGIN { print 5 % 3 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["2".to_string()]);
    }

    #[test]
    fn eval_print_numeric_div_infix_expression() {
        let lexer = Lexer::new(r#"BEGIN { print 5 / 5 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["1".to_string()]);
    }

    #[test]
    fn eval_print_numeric_caret_infix_expression() {
        let lexer = Lexer::new(r#"BEGIN { print 2 ^ 3 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["8".to_string()]);
    }

    #[test]
    fn eval_print_numeric_minus_infix_expression() {
        let lexer = Lexer::new(r#"BEGIN { print 5 - 3 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["2".to_string()]);
    }

    #[test]
    fn eval_print_string_and_number_expressions() {
        let lexer = Lexer::new(r#"BEGIN { print "Value:" 42 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["Value:42".to_string()]);
    }

    #[test]
    fn eval_print_expression_with_parantheses() {
        let lexer = Lexer::new(r#"BEGIN { print (1 + 2) * 3 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["9".to_string()]);
    }

    #[test]
    fn eval_print_multiplication_has_higher_precedence_than_addition() {
        let lexer = Lexer::new(r#"BEGIN { print 1 + 2 * 3 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["7".to_string()]);
    }

    #[test]
    fn eval_print_power_is_right_associative() {
        let lexer = Lexer::new(r#"BEGIN { print 2 ^ 3 ^ 2 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["512".to_string()]);
    }

    #[test]
    fn eval_print_minus_is_left_associative() {
        let lexer = Lexer::new(r#"BEGIN { print 5 - 3 - 1 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["1".to_string()]);
    }

    #[test]
    fn eval_print_ternary_expression_selects_correct_branch() {
        let lexer = Lexer::new(r#"{ print ($1 > 10) ? "big" : "small" }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["12".to_string(), "7".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["big".to_string(), "small".to_string()]);
    }

    #[test]
    fn eval_print_field_zero_returns_entire_line() {
        let lexer = Lexer::new(r#"{ print $0 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["one two".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["one two".to_string()]);
    }

    #[test]
    fn eval_print_field_first_column() {
        let lexer = Lexer::new(r#"{ print $1, $3 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["one     two three".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["one three".to_string()]);
    }

    #[test]
    fn eval_print_number_of_fields_identifier() {
        let lexer = Lexer::new(r#"{ print NF, $1 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["one two three".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["3 one".to_string()]);
    }

    #[test]
    fn eval_print_number_of_fields_on_empty_line() {
        let lexer = Lexer::new(r#"{ print NF }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["0".to_string()]);
    }

    #[test]
    fn eval_print_uninitialized_identifier() {
        let lexer = Lexer::new(r#"{ print XYZ }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["one two".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["".to_string()]);
    }

    #[test]
    fn eval_print_use_number_of_fields_in_field_expression() {
        let lexer = Lexer::new(r#"{ print $NF }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["one two three".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["three".to_string()]);
    }

    #[test]
    fn eval_chained_field_assignment_updates_record_right_to_left() {
        let lexer = Lexer::new(
            r#"{ $1 = $0 = $2; print } { $0 = $2 = $1; print } { $(0) = $(2) = $(1); print }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(
            program,
            vec![
                "left right".to_string(),
                "alpha beta".to_string(),
                "gamma delta".to_string(),
            ],
            "-",
        );

        let output = evaluator.eval();

        assert_eq!(
            output,
            vec![
                "right".to_string(),
                "right".to_string(),
                "right".to_string(),
                "beta".to_string(),
                "beta".to_string(),
                "beta".to_string(),
                "delta".to_string(),
                "delta".to_string(),
                "delta".to_string(),
            ]
        );
    }

    #[test]
    fn eval_print_line_numbers() {
        let lexer = Lexer::new(r#"{ print NR, $0 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator =
            Evaluator::new(program, vec!["one".to_string(), "two".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["1 one".to_string(), "2 two".to_string()]);
    }

    #[test]
    fn eval_printf_with_width_and_alignment() {
        let lexer = Lexer::new(r#"{ printf "[%10s] [%-16d]\n", $1, $3 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["USSR 8649 275 Asia".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["[      USSR] [275             ]".to_string()]);
    }

    #[test]
    fn eval_gsub_then_print_uses_updated_line() {
        let lexer = Lexer::new(r#"{ gsub(/USA/, "United States"); print }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator =
            Evaluator::new(program, vec!["USA 3615 237 North America".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(
            output,
            vec!["United States 3615 237 North America".to_string()]
        );
    }

    #[test]
    fn eval_gsub_with_target_variable_updates_variable_only() {
        let lexer = Lexer::new(r#"{ t = $0; gsub(/[ \t]+/, "", t); print t; print $0 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["a b\tc".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["abc".to_string(), "a b\tc".to_string()]);
    }

    #[test]
    fn eval_gsub_string_pattern_uses_awk_replacement_semantics() {
        let lexer = Lexer::new(
            r#"{ gsub("[" $1 "]", "(&)"); print } { gsub("[" $1 "]", "(\\&)"); print }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["abc\txyz".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(
            output,
            vec![
                "(a)(b)(c)\txyz".to_string(),
                "(&)(&)(&)(&)(&)(&)(&)(&)(&)\txyz".to_string()
            ]
        );
    }

    #[test]
    fn eval_nf_with_non_space_fs_counts_empty_record_as_zero_fields() {
        let lexer = Lexer::new("BEGIN { FS=\":\"; OFS=\":\" } { print NF \"	\", $0 }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator =
            Evaluator::new(program, vec![String::new(), "/dev/rrp3:".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(
            output,
            vec!["0\t:".to_string(), "2\t:/dev/rrp3:".to_string()]
        );
    }

    #[test]
    fn eval_regex_fs_splits_fields_for_print() {
        let lexer = Lexer::new("BEGIN { FS = \"\\t+\" } { print $1, $2 }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["17379\tmel\t".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["17379 mel".to_string()]);
    }

    #[test]
    fn eval_print_length_and_line() {
        let lexer = Lexer::new(r#"{ print length, $0 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["USSR 8649 275 Asia".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["18 USSR 8649 275 Asia".to_string()]);
    }

    #[test]
    fn eval_field_assignment_with_substr_then_print() {
        let lexer = Lexer::new(r#"{ $1 = substr($1, 1, 3); print }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(
            program,
            vec!["Canada\t3852\t25\tNorth America".to_string()],
            "-",
        );

        let output = evaluator.eval();

        assert_eq!(output, vec!["Can 3852 25 North America".to_string()]);
    }

    #[test]
    fn eval_assignment_with_concatenation() {
        let lexer = Lexer::new(r#"{ s = s " " substr($1, 1, 3) } END { print s }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(
            program,
            vec![
                "USSR\t8649\t275\tAsia".to_string(),
                "Canada\t3852\t25\tNorth America".to_string(),
            ],
            "-",
        );

        let output = evaluator.eval();

        assert_eq!(output, vec![" USS Can".to_string()]);
    }

    #[test]
    fn eval_chained_assignment_sets_fs_and_ofs() {
        let lexer = Lexer::new(r#"BEGIN { FS = OFS = "\t" } { $4 = "NA"; print }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(
            program,
            vec!["Canada\t3852\t25\tNorth America".to_string()],
            "-",
        );

        let output = evaluator.eval();

        assert_eq!(output, vec!["Canada\t3852\t25\tNA".to_string()]);
    }

    #[test]
    fn eval_if_statement_updates_variables() {
        let lexer = Lexer::new(
            r#"{ if (maxpop < $3) { maxpop = $3; country = $1 } } END { print country, maxpop }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(
            program,
            vec![
                "USSR\t8649\t275\tAsia".to_string(),
                "China\t3705\t1032\tAsia".to_string(),
            ],
            "-",
        );

        let output = evaluator.eval();

        assert_eq!(output, vec!["China 1032".to_string()]);
    }

    #[test]
    fn eval_while_with_post_increment() {
        let lexer = Lexer::new(r#"{ i = 1; while (i <= NF) { print $i; i++ } }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["USSR\t8649\t275\tAsia".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(
            output,
            vec![
                "USSR".to_string(),
                "8649".to_string(),
                "275".to_string(),
                "Asia".to_string(),
            ]
        );
    }

    #[test]
    fn eval_do_while_with_post_increment() {
        let lexer = Lexer::new(r#"{ i = 1; s = ""; do { s = s $i } while (i++ < NF) print s }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["USSR\t8649\t275\tAsia".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["USSR8649275Asia".to_string()]);
    }

    #[test]
    fn eval_while_break_clears_loop_flag_for_following_statements() {
        let lexer = Lexer::new(r#"BEGIN { while (1) { print "loop"; break } print "after" }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, Vec::new(), "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["loop".to_string(), "after".to_string()]);
    }

    #[test]
    fn eval_while_continue_advances_and_clears_loop_flag() {
        let lexer = Lexer::new(
            r#"BEGIN { i = 0; while (i < 3) { i++; if (i < 3) continue; print i } print "after" }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, Vec::new(), "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["3".to_string(), "after".to_string()]);
    }

    #[test]
    fn eval_do_while_break_clears_loop_flag_for_following_statements() {
        let lexer = Lexer::new(r#"BEGIN { do { print "loop"; break } while (1); print "after" }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, Vec::new(), "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["loop".to_string(), "after".to_string()]);
    }

    #[test]
    fn eval_do_while_continue_advances_and_clears_loop_flag() {
        let lexer = Lexer::new(
            r#"BEGIN { i = 0; do { i++; if (i < 3) continue; print i } while (i < 3); print "after" }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, Vec::new(), "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["3".to_string(), "after".to_string()]);
    }

    #[test]
    fn eval_for_with_post_increment() {
        let lexer = Lexer::new(r#"{ for (i = 1; i <= NF; i++) print $i }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["USSR\t8649\t275\tAsia".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(
            output,
            vec![
                "USSR".to_string(),
                "8649".to_string(),
                "275".to_string(),
                "Asia".to_string(),
            ]
        );
    }

    #[test]
    fn eval_continue_skips_to_next_for_iteration() {
        let lexer = Lexer::new(
            r#"{ for (i = 1; i <= NF; i++) { if ($i ~ /^[0-9]+$/) continue; print $i } }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["abc 123 def 456".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["abc".to_string(), "def".to_string()]);
    }

    #[test]
    fn eval_exit_stops_processing_and_preserves_nr_for_end() {
        let lexer = Lexer::new(
            r#"NR >= 2 { exit } END { if (NR < 2) print FILENAME " has only " NR " lines" }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(
            program,
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            "-",
        );

        let output = evaluator.eval();

        assert_eq!(output, Vec::<String>::new());
    }

    #[test]
    fn eval_user_defined_function_call_can_exit() {
        let lexer = Lexer::new(
            r#"BEGIN { print "before"; myabort(1); print "after" } function myabort(n) { print "exit", n; exit n } END { print "end" }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["A".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(
            output,
            vec![
                "before".to_string(),
                "exit 1".to_string(),
                "end".to_string()
            ]
        );
    }

    #[test]
    fn eval_array_add_assignment_and_access() {
        let lexer = Lexer::new(r#"/Asia/ { pop["Asia"] += $3 } END { print pop["Asia"] }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(
            program,
            vec![
                "USSR\t8649\t275\tAsia".to_string(),
                "China\t3705\t1032\tAsia".to_string(),
            ],
            "-",
        );

        let output = evaluator.eval();

        assert_eq!(output, vec!["1307".to_string()]);
    }

    #[test]
    fn eval_delete_removes_array_entry_from_for_in() {
        let lexer =
            Lexer::new(r#"BEGIN { x[1] = "a"; x[2] = "b"; delete x[1]; for (i in x) print i }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["2".to_string()]);
    }

    #[test]
    fn eval_for_in_loop_over_array_keys() {
        let lexer = Lexer::new(
            r#"BEGIN { area["Asia"] = 1; area["Europe"] = 2 } END { for (name in area) print name ":" area[name] }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["Asia:1".to_string(), "Europe:2".to_string()]);
    }

    #[test]
    fn eval_parenthesized_composite_membership_expression() {
        let lexer =
            Lexer::new(r#"{ x[$0, $1] = "hit"; if (($0, $1) in x) print "yes"; else print "no" }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["17379\tmel".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["yes".to_string()]);
    }

    #[test]
    fn eval_print_respects_ors_between_records() {
        let lexer = Lexer::new(r#"BEGIN { OFS = ":"; ORS = "\n\n" } { print $1, $2 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(
            program,
            vec![
                "USSR\t8649\t275\tAsia".to_string(),
                "Canada\t3852\t25\tNorth America".to_string(),
            ],
            "-",
        );

        let output = evaluator.eval();

        assert_eq!(
            output,
            vec![
                "USSR:8649".to_string(),
                "".to_string(),
                "Canada:3852".to_string(),
                "".to_string()
            ]
        );
    }

    #[test]
    fn eval_print_redirection_does_not_write_stdout() {
        let lexer = Lexer::new(r#"{ print >"tempbig" }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["USSR\t8649\t275\tAsia".to_string()], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
    }

    #[test]
    fn eval_print_pipe_to_sort_flushes_sorted_output() {
        let lexer = Lexer::new(r#"BEGIN { print "b" | "sort"; print "a" | "sort" }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["a".to_string(), "b".to_string()]);
    }

    #[test]
    fn eval_split_function_call_updates_array_and_returns_count() {
        let lexer = Lexer::new(
            r#"function f(a) { return split($0, a) } { print f(x); print x[1]; print x[2] }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["hello world".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(
            output,
            vec!["2".to_string(), "hello".to_string(), "world".to_string()]
        );
    }

    #[test]
    fn eval_getline_in_begin_consumes_input_records() {
        let lexer = Lexer::new(r#"BEGIN { while (getline && NR < 3) print } { print }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(
            program,
            vec!["A".to_string(), "B".to_string(), "C".to_string()],
            "-",
        );

        let output = evaluator.eval();

        assert_eq!(output, vec!["A".to_string(), "B".to_string()]);
    }

    #[test]
    fn unescape_awk_string_handles_known_escapes() {
        let input = r#"line1\nline2\t\"x\"\\done\r"#;

        let output = unescape_awk_string(input);

        assert_eq!(output, "line1\nline2\t\"x\"\\done\r");
    }

    #[test]
    fn unescape_awk_string_preserves_unknown_escape_sequences() {
        let input = r#"a\qb\zc"#;

        let output = unescape_awk_string(input);

        assert_eq!(output, r#"a\qb\zc"#);
    }

    #[test]
    fn unescape_awk_string_keeps_trailing_backslash() {
        let input = "abc\\";

        let output = unescape_awk_string(input);

        assert_eq!(output, "abc\\");
    }

    #[test]
    fn unescape_awk_string_mixes_plain_and_escaped_text() {
        let input = r#"A\tB\nC\\D\"E\q"#;

        let output = unescape_awk_string(input);

        assert_eq!(output, "A\tB\nC\\D\"E\\q");
    }

    #[test]
    fn eval_print_string_with_embedded_newline_splits_output_lines() {
        let lexer = Lexer::new(r#"BEGIN { print "\nUSSR" }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["".to_string(), "USSR".to_string()]);
    }

    #[test]
    fn eval_print_respects_custom_ors_without_newlines() {
        let lexer = Lexer::new(r###"BEGIN { ORS = "##" } { print $1; print $2 }"###);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["alpha beta".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["alpha##beta##".to_string()]);
    }

    #[test]
    fn eval_match_function_sets_rstart_and_rlength() {
        let lexer = Lexer::new(r#"BEGIN { print match("abc123", "[0-9]+"), RSTART, RLENGTH }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["4 4 3".to_string()]);
    }

    #[test]
    fn eval_match_function_falls_back_to_legacy_regex_matching() {
        let lexer = Lexer::new(r#"BEGIN { print match("daemon", "(foo|dae)") , RSTART, RLENGTH }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["1 1 3".to_string()]);
    }

    #[test]
    fn eval_dynamic_regex_string_uses_full_regex_semantics() {
        let lexer = Lexer::new(
            r#"BEGIN { r = "[^aeiou]"; p = "17:"; if ("17abc" ~ r) print "class"; if ("17abc" ~ p) print "prefix"; if ("17:rest" ~ p) print "colon" }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["class".to_string(), "colon".to_string()]);
    }

    #[test]
    fn eval_comparison_distinguishes_uninitialized_vars_from_empty_fields() {
        let lexer = Lexer::new(
            r#"BEGIN { FS = ":" } { if (b == 0) print "b"; if ($1 == 0) print "$1num"; if ($1 == "") print "$1str" }"#,
        );
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![":x".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["b".to_string(), "$1str".to_string()]);
    }

    #[test]
    fn eval_field_with_string_literal_index_returns_empty() {
        // eval_numeric_expression returns None for a bare string literal,
        // so $"abc" yields "" rather than coercing to $0 as strict AWK would.
        let lexer = Lexer::new(r#"{ print $"abc" }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["hello world".to_string()], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["".to_string()]);
    }

    #[test]
    fn eval_field_in_begin_block_returns_empty() {
        // No current line exists in a BEGIN block, so all field accesses return "".
        let lexer = Lexer::new("BEGIN { print $1 }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["".to_string()]);
    }

    #[test]
    fn eval_negative_field_index_sets_runtime_error_and_returns_no_output() {
        let lexer = Lexer::new("{ print $(-1) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["hello world".to_string()], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert_eq!(evaluator.runtime_error(), Some("attempt to access field -1"));
    }

    #[test]
    fn eval_division_by_zero_sets_runtime_error() {
        let lexer = Lexer::new("BEGIN { print 1 / 0 }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert_eq!(evaluator.runtime_error(), Some("division by zero"));
    }

    #[test]
    fn eval_modulo_by_zero_sets_runtime_error() {
        let lexer = Lexer::new("BEGIN { print 5 % 0 }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert_eq!(evaluator.runtime_error(), Some("division by zero"));
    }

    #[test]
    fn eval_divide_assign_by_zero_sets_runtime_error() {
        let lexer = Lexer::new("BEGIN { x = 4; x /= 0; print x }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert_eq!(evaluator.runtime_error(), Some("division by zero"));
    }

    #[test]
    fn eval_modulo_assign_by_zero_sets_runtime_error() {
        let lexer = Lexer::new("BEGIN { x = 4; x %= 0; print x }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert_eq!(evaluator.runtime_error(), Some("division by zero"));
    }

    #[test]
    fn eval_gsub_with_invalid_regex_sets_runtime_error() {
        let lexer = Lexer::new(r#"{ gsub("[invalid", "x"); print }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["hello".to_string()], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert!(
            evaluator
                .runtime_error()
                .is_some_and(|e| e.starts_with("invalid regex in gsub:")),
            "unexpected error: {:?}",
            evaluator.runtime_error()
        );
    }

    #[test]
    fn eval_sub_with_invalid_regex_sets_runtime_error() {
        let lexer = Lexer::new(r#"{ sub("[invalid", "x"); print }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec!["hello".to_string()], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert!(
            evaluator
                .runtime_error()
                .is_some_and(|e| e.starts_with("invalid regex in sub:")),
            "unexpected error: {:?}",
            evaluator.runtime_error()
        );
    }

    #[test]
    fn eval_unhandled_operator_in_expression_sets_runtime_error() {
        // && in expression context (outside a condition) reaches the _ => None
        // wildcard in eval_numeric_infix, which previously returned "not implemented".
        let lexer = Lexer::new("BEGIN { print (1 && 0) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert!(
            evaluator
                .runtime_error()
                .is_some_and(|e| e.contains("cannot be applied to given operands")),
            "unexpected error: {:?}",
            evaluator.runtime_error()
        );
    }

    #[test]
    fn eval_sqrt_of_negative_sets_runtime_error() {
        let lexer = Lexer::new("BEGIN { print sqrt(-1) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert_eq!(
            evaluator.runtime_error(),
            Some("sqrt called with negative argument -1")
        );
    }

    #[test]
    fn eval_log_of_zero_sets_runtime_error() {
        let lexer = Lexer::new("BEGIN { print log(0) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert_eq!(
            evaluator.runtime_error(),
            Some("log called with non-positive argument 0")
        );
    }

    #[test]
    fn eval_log_of_negative_sets_runtime_error() {
        let lexer = Lexer::new("BEGIN { print log(-5) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert!(output.is_empty());
        assert_eq!(
            evaluator.runtime_error(),
            Some("log called with non-positive argument -5")
        );
    }

    #[test]
    fn eval_printf_with_no_arguments_sets_runtime_error() {
        let mut evaluator = Evaluator::new(Program::new(), vec![], "-");
        let statement = Statement::Printf(vec![]);

        let output = evaluator.eval_statement(&statement, None);

        assert!(output.is_empty());
        assert_eq!(
            evaluator.runtime_error(),
            Some("printf requires at least a format string")
        );
    }

    #[test]
    fn eval_sin_of_zero_returns_zero() {
        let lexer = Lexer::new("BEGIN { print sin(0) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["0".to_string()]);
    }

    #[test]
    fn eval_sin_of_pi_over_two_returns_one() {
        let lexer = Lexer::new("BEGIN { print sin(3.14159265358979 / 2) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["1".to_string()]);
    }

    #[test]
    fn eval_cos_of_zero_returns_one() {
        let lexer = Lexer::new("BEGIN { print cos(0) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["1".to_string()]);
    }

    #[test]
    fn eval_cos_of_pi_returns_minus_one() {
        let lexer = Lexer::new("BEGIN { print cos(3.14159265358979) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["-1".to_string()]);
    }

    #[test]
    fn eval_srand_sets_seed_and_returns_it() {
        let lexer = Lexer::new("BEGIN { print srand(42) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["42".to_string()]);
    }

    #[test]
    fn eval_srand_affects_subsequent_rand_output() {
        let lexer = Lexer::new("BEGIN { srand(1); r1 = rand(); srand(1); r2 = rand(); print (r1 == r2) }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["1".to_string()]);
    }

    #[test]
    fn eval_numeric_function_call_int_truncates_positive_float() {
        let lexer = Lexer::new("BEGIN { print int(3.9) + 0 }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["3".to_string()]);
    }

    #[test]
    fn eval_numeric_function_call_int_truncates_negative_float() {
        let lexer = Lexer::new("BEGIN { print int(-2.9) + 0 }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["-2".to_string()]);
    }

    #[test]
    fn eval_numeric_function_call_exp_of_zero_returns_one() {
        let lexer = Lexer::new("BEGIN { print exp(0) + 0 }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["1".to_string()]);
    }

    #[test]
    fn eval_numeric_function_call_sqrt_of_perfect_square() {
        let lexer = Lexer::new("BEGIN { print sqrt(9) + 0 }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["3".to_string()]);
    }

    #[test]
    fn eval_numeric_function_call_log_of_one_returns_zero() {
        let lexer = Lexer::new("BEGIN { print log(1) + 0 }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["0".to_string()]);
    }

    #[test]
    fn eval_numeric_function_call_index_returns_one_based_position() {
        let lexer = Lexer::new(r#"BEGIN { print index("foobar", "bar") + 0 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["4".to_string()]);
    }

    #[test]
    fn eval_numeric_function_call_index_returns_zero_when_not_found() {
        let lexer = Lexer::new(r#"BEGIN { print index("foobar", "xyz") + 0 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["0".to_string()]);
    }

    #[test]
    fn eval_numeric_function_call_split_returns_field_count_in_arithmetic() {
        let lexer = Lexer::new(r#"BEGIN { n = split("a:b:c", arr, ":"); print n + 0 }"#);
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["3".to_string()]);
    }

    #[test]
    fn eval_array_post_decrement_decreases_existing_value() {
        let lexer = Lexer::new("BEGIN { a[1] = 5; a[1]--; print a[1] }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["4".to_string()]);
    }

    #[test]
    fn eval_array_post_decrement_initializes_from_zero() {
        let lexer = Lexer::new("BEGIN { a[\"x\"]--; print a[\"x\"] }");
        let mut parser = Parser::new(lexer);
        let program = parser.parse_program();
        let mut evaluator = Evaluator::new(program, vec![], "-");

        let output = evaluator.eval();

        assert_eq!(output, vec!["-1".to_string()]);
    }
}
