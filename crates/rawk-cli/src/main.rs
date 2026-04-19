use std::io::{BufRead, IsTerminal};
use std::{io, path};

use clap::{CommandFactory, Parser};
use rawk_core::awk::Awk;

#[derive(Parser, Debug)]
struct Args {
    /// Program text is read from file instead of command line
    #[arg(short = 'f', long = "file", value_name = "program-file")]
    program_file: Option<path::PathBuf>,

    /// Use fs as the input field separator
    #[arg(short = 'F', long = "field-separator", value_name = "fs")]
    field_separator: Option<String>,

    /// Positional arguments: PROGRAM INPUT or INPUT when using -f
    #[arg(value_name = "ARGS", num_args = 0..=2)]
    args: Vec<String>,
}

fn main() -> io::Result<()> {
    let args = Args::parse();

    let (script, input) = if let Some(program_file) = args.program_file {
        let script = std::fs::read_to_string(program_file)?;
        match args.args.as_slice() {
            [input] => (script, input.clone()),
            _ => {
                // No input file provided, read from stdin
                run_stdin(&script, args.field_separator);

                return Ok(());
            }
        }
    } else {
        match args.args.as_slice() {
            [script, input] => (script.clone(), input.clone()),
            [script] => {
                // No input file provided, read from stdin
                run_stdin(script, args.field_separator);

                return Ok(());
            }
            _ => {
                let mut cmd = Args::command();
                cmd.print_help()?;
                println!();
                return Ok(());
            }
        }
    };

    execute(&script, path::Path::new(&input), args.field_separator)?;

    Ok(())
}

fn execute(script: &str, path: &path::Path, field_separator: Option<String>) -> io::Result<()> {
    let file = std::fs::File::open(path).expect("Failed to read input file");
    let input_lines = io::BufReader::new(file)
        .lines()
        .map_while(Result::ok);

    let awk = Awk::new(script)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err.to_string()))?;
    let filename = display_filename(path);
    let stdout = io::stdout();
    let runtime_error = awk.run_to_writer(input_lines, Some(filename), field_separator, Box::new(stdout));

    if let Some(err) = runtime_error {
        eprintln!("rawk: {err}");
    }

    Ok(())
}

fn display_filename(path: &path::Path) -> String {
    let relative = std::env::current_dir()
        .ok()
        .and_then(|cwd| path.strip_prefix(cwd).ok().map(path::Path::to_path_buf))
        .unwrap_or_else(|| path.to_path_buf());

    relative.to_string_lossy().replace('\\', "/")
}

fn run_stdin(script: &str, field_separator: Option<String>) {
    if io::stdin().is_terminal() {
        interactive_mode(script, field_separator);
    } else {
        stdin_mode(script, field_separator);
    }
}

fn stdin_mode(script: &str, field_separator: Option<String>) {
    let awk = match Awk::new(script) {
        Ok(awk) => awk,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };

    let stdin = io::stdin();
    let input_lines: Vec<String> = stdin.lock().lines().map_while(Result::ok).collect();

    let (output_lines, runtime_error) = awk.run(input_lines, None, field_separator);

    for line in output_lines {
        println!("{}", line);
    }

    if let Some(err) = runtime_error {
        eprintln!("rawk: {err}");
    }
}

fn interactive_mode(script: &str, field_separator: Option<String>) {
    use std::io::Write;

    let awk = match Awk::new(script) {
        Ok(awk) => awk,
        Err(err) => {
            eprintln!("{err}");
            return;
        }
    };
    let mut input = String::new();

    loop {
        io::stdout().flush().unwrap();

        input.clear();
        match io::stdin().read_line(&mut input) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }

        let (output_lines, runtime_error) =
            awk.run(vec![input.trim().to_string()], None, field_separator.clone());

        for line in output_lines {
            println!("{}", line);
        }

        if let Some(err) = runtime_error {
            eprintln!("rawk: {err}");
        }
    }
}
