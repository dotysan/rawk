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
                // No input file provided only script, enter interactive mode
                interactive_mode(&script, args.field_separator);

                return Ok(());
            }
        }
    } else {
        match args.args.as_slice() {
            [script, input] => (script.clone(), input.clone()),
            [script] => {
                // No input file provided only script, enter interactive mode
                interactive_mode(script, args.field_separator);

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
    let input_lines = std::fs::read_to_string(path)
        .expect("Failed to read input file")
        .lines()
        .map(|line| line.to_string())
        .collect::<Vec<String>>();

    let awk = Awk::new(script)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidInput, err.to_string()))?;
    let filename = display_filename(path);
    let (output_lines, runtime_error) = awk.run(input_lines, Some(filename), field_separator);

    for line in output_lines {
        println!("{}", line);
    }

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
