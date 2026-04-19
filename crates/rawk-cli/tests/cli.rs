use std::io::Write;
use std::process::Command;

fn run_rawk(script: &str) -> std::process::Output {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/emp.data");
    let rawk = env!("CARGO_BIN_EXE_rawk");

    Command::new(rawk)
        .arg(script)
        .arg(&path)
        .output()
        .expect("failed to run rawk")
}

fn run_rawk_with_fs(flag: &str, fs: &str, script: &str) -> std::process::Output {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/emp.csv");
    let rawk = env!("CARGO_BIN_EXE_rawk");

    Command::new(rawk)
        .arg(flag)
        .arg(fs)
        .arg(script)
        .arg(&path)
        .output()
        .expect("failed to run rawk")
}

fn run_rawk_from_file() -> std::process::Output {
    let path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/emp.data");
    let script_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/print.awk");
    let rawk = env!("CARGO_BIN_EXE_rawk");

    Command::new(rawk)
        .arg("-f")
        .arg(&script_path)
        .arg(&path)
        .output()
        .expect("failed to run rawk")
}

fn run_rawk_no_args() -> std::process::Output {
    let rawk = env!("CARGO_BIN_EXE_rawk");

    Command::new(rawk).output().expect("failed to run rawk")
}

fn run_rawk_with_input(script: &str, input: &str) -> std::process::Output {
    let rawk = env!("CARGO_BIN_EXE_rawk");

    let mut child = Command::new(rawk)
        .arg(script)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("failed to run rawk");

    child
        .stdin
        .as_mut()
        .expect("failed to open stdin")
        .write_all(input.as_bytes())
        .expect("failed to write to stdin");

    child.wait_with_output().expect("failed to read stdout")
}

#[test]
fn print_identity_outputs_input_lines() {
    let script = "{ print }";

    let output = run_rawk(script);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Beth    4.00    0\n"));
    assert!(output.stderr.is_empty());
}

#[test]
fn print_identity_outputs_input_lines_from_file() {
    let output = run_rawk_from_file();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(String::from_utf8_lossy(&output.stdout).contains("Beth    4.00    0\n"));
    assert!(output.stderr.is_empty());
}

#[test]
fn print_begin_end_blocks_with_expressions() {
    let script = "BEGIN { print 1+1 } END { print 3 + 3}";

    let output = run_rawk(script);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("2"));
    assert_eq!(lines.next(), Some("6"));
    assert!(output.stderr.is_empty());
}

#[test]
fn print_begin_rules_and_end_blocks_with_expressions() {
    let script = "BEGIN { print 10 } { print $1 } END { print 20 }";

    let output = run_rawk(script);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("10"));
    assert_eq!(lines.next(), Some("Beth"));
    assert_eq!(lines.next(), Some("Dan"));
    assert_eq!(lines.next(), Some("Kathy"));
    assert_eq!(lines.next(), Some("Mark"));
    assert_eq!(lines.next(), Some("Mary"));
    assert_eq!(lines.next(), Some("Susie"));
    assert_eq!(lines.next(), Some("20"));
    assert!(output.stderr.is_empty());
}

#[test]
fn help_shows_when_no_args_given() {
    let output = run_rawk_no_args();

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(stdout.contains("Usage:"), "stdout: {stdout}");
    assert!(stdout.contains("ARGS"), "stdout: {stdout}");
    assert!(stdout.contains("file"), "stdout: {stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn print_filename() {
    let script = "END { print FILENAME }";

    let output = run_rawk(script);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("tests/emp.data"));
    assert!(output.stderr.is_empty());
}

#[test]
fn system_statement_in_script_does_not_break_cli_execution() {
    let script = r#"{ system("echo ignored"); print $1 }"#;

    let output = run_rawk(script);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("Beth"));
    assert_eq!(lines.next(), Some("Dan"));
    assert_eq!(lines.next(), Some("Kathy"));
    assert_eq!(lines.next(), Some("Mark"));
    assert_eq!(lines.next(), Some("Mary"));
    assert_eq!(lines.next(), Some("Susie"));
    assert!(lines.next().is_none(), "stdout: {stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn negative_field_index_prints_error_to_stderr() {
    let output = run_rawk("{ print $(-1) }");

    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("attempt to access field -1"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn division_by_zero_prints_error_to_stderr() {
    let output = run_rawk("{ print $1 / 0 }");

    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("division by zero"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sqrt_of_negative_prints_error_to_stderr() {
    let output = run_rawk("BEGIN { print sqrt(-1) }");

    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("sqrt called with negative argument"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn log_of_non_positive_prints_error_to_stderr() {
    let output = run_rawk("BEGIN { print log(0) }");

    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("log called with non-positive argument"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn gsub_with_invalid_regex_prints_error_to_stderr() {
    let output = run_rawk(r#"{ gsub("[invalid", "x"); print }"#);

    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid regex in gsub:"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sub_with_invalid_regex_prints_error_to_stderr() {
    let output = run_rawk(r#"{ sub("[invalid", "x"); print }"#);

    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("invalid regex in sub:"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn unhandled_operator_in_expression_prints_error_to_stderr() {
    let output = run_rawk("BEGIN { print (1 && 0) }");

    assert!(output.stdout.is_empty());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("cannot be applied to given operands"),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn field_separator_short_flag_splits_fields() {
    let output = run_rawk_with_fs("-F", ",", "{ print $1 }");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("Beth"));
    assert_eq!(lines.next(), Some("Dan"));
    assert_eq!(lines.next(), Some("Kathy"));
    assert_eq!(lines.next(), Some("Mark"));
    assert_eq!(lines.next(), Some("Mary"));
    assert_eq!(lines.next(), Some("Susie"));
    assert!(lines.next().is_none(), "stdout: {stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn field_separator_long_flag_splits_fields() {
    let output = run_rawk_with_fs("--field-separator", ",", "{ print $2 }");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("4.00"));
    assert_eq!(lines.next(), Some("3.75"));
    assert_eq!(lines.next(), Some("4.00"));
    assert_eq!(lines.next(), Some("5.00"));
    assert_eq!(lines.next(), Some("5.50"));
    assert_eq!(lines.next(), Some("4.25"));
    assert!(lines.next().is_none(), "stdout: {stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn sub_with_target_replaces_first_occurrence() {
    let script = r#"{ sub(/foo/, "bar", $0); print }"#;

    let output = run_rawk_with_input(script, "foo foo foo\n");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "bar foo foo");
    assert!(output.stderr.is_empty());
}

#[test]
fn gsub_with_target_replaces_all_occurrences() {
    let script = r#"{ gsub(/foo/, "bar", $0); print }"#;

    let output = run_rawk_with_input(script, "foo foo foo\n");

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert_eq!(stdout.trim(), "bar bar bar");
    assert!(output.stderr.is_empty());
}
