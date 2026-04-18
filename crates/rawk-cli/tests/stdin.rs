use std::io::Write;
use std::process::{Command, Stdio};

fn run_rawk_stdin(script: &str, input: &[u8]) -> std::process::Output {
    let rawk = env!("CARGO_BIN_EXE_rawk");
    let mut child = Command::new(rawk)
        .arg(script)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn rawk");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        stdin.write_all(input).expect("failed to write stdin");
    }

    child.wait_with_output().expect("failed to wait on rawk")
}

#[test]
fn stdin_preserves_variable_state_across_lines() {
    let script = "$1 != prev { print; prev = $1 }";
    let input = b"1 1\n1 2\n2 3\n2 4\n3 5\n3 6\n";

    let output = run_rawk_stdin(script, input);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("1 1"));
    assert_eq!(lines.next(), Some("2 3"));
    assert_eq!(lines.next(), Some("3 5"));
    assert!(lines.next().is_none(), "stdout: {stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn stdin_supports_begin_and_end_blocks() {
    let script = "BEGIN { print \"start\" } { print $1 } END { print \"done\" }";
    let input = b"alpha 1\nbeta 2\n";

    let output = run_rawk_stdin(script, input);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("start"));
    assert_eq!(lines.next(), Some("alpha"));
    assert_eq!(lines.next(), Some("beta"));
    assert_eq!(lines.next(), Some("done"));
    assert!(lines.next().is_none(), "stdout: {stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn stdin_nr_increments_across_lines() {
    let script = "{ print NR, $0 }";
    let input = b"first\nsecond\nthird\n";

    let output = run_rawk_stdin(script, input);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("1 first"));
    assert_eq!(lines.next(), Some("2 second"));
    assert_eq!(lines.next(), Some("3 third"));
    assert!(lines.next().is_none(), "stdout: {stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn stdin_accumulates_across_lines() {
    let script = "{ sum += $1 } END { print sum }";
    let input = b"10\n20\n30\n";

    let output = run_rawk_stdin(script, input);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("60"));
    assert!(lines.next().is_none(), "stdout: {stdout}");
    assert!(output.stderr.is_empty());
}

fn run_rawk_stdin_from_file(script_path: &str, input: &[u8]) -> std::process::Output {
    let rawk = env!("CARGO_BIN_EXE_rawk");
    let mut child = Command::new(rawk)
        .arg("-f")
        .arg(script_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("failed to spawn rawk");

    {
        let stdin = child.stdin.as_mut().expect("failed to open stdin");
        stdin.write_all(input).expect("failed to write stdin");
    }

    child.wait_with_output().expect("failed to wait on rawk")
}

fn run_rawk_file(script: &str, data_path: &str) -> std::process::Output {
    let rawk = env!("CARGO_BIN_EXE_rawk");

    Command::new(rawk)
        .arg(script)
        .arg(data_path)
        .output()
        .expect("failed to run rawk")
}

#[test]
fn stdin_from_file_flag_preserves_state() {
    let script_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/dedupe_sorted.awk");
    let input = b"1 a\n1 b\n2 c\n2 d\n3 e\n";

    let output = run_rawk_stdin_from_file(script_path, input);

    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut lines = stdout.lines();

    assert_eq!(lines.next(), Some("1 a"));
    assert_eq!(lines.next(), Some("2 c"));
    assert_eq!(lines.next(), Some("3 e"));
    assert!(lines.next().is_none(), "stdout: {stdout}");
    assert!(output.stderr.is_empty());
}

#[test]
fn stdin_and_file_produce_identical_output() {
    let data_path = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/emp.data");
    let script = "{ print $1 }";

    let file_output = run_rawk_file(script, data_path);
    assert!(
        file_output.status.success(),
        "file stderr: {}",
        String::from_utf8_lossy(&file_output.stderr)
    );

    let file_bytes = std::fs::read(data_path).expect("failed to read test data");
    let stdin_output = run_rawk_stdin(script, &file_bytes);
    assert!(
        stdin_output.status.success(),
        "stdin stderr: {}",
        String::from_utf8_lossy(&stdin_output.stderr)
    );

    assert_eq!(
        String::from_utf8_lossy(&file_output.stdout),
        String::from_utf8_lossy(&stdin_output.stdout),
        "file and stdin output differ"
    );
}
