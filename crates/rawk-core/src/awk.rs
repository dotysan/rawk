use crate::{Evaluator, Lexer, ParseError, Parser, Program};
use std::io::Write;

/// High-level wrapper for compiling and running an AWK script.
///
/// The script is parsed once on construction and can then be executed against
/// any number of independent inputs without re-parsing.
///
/// # Examples
///
/// Print every line of input unchanged:
///
/// ```
/// use rawk_core::awk::Awk;
///
/// let awk = Awk::new("{ print }").unwrap();
/// let (output, error) = awk.run(vec!["hello world".into()], None, None);
/// assert_eq!(output, vec!["hello world".to_string()]);
/// assert!(error.is_none());
/// ```
///
/// Supply a filename so that the `FILENAME` built-in variable is populated:
///
/// ```
/// use rawk_core::awk::Awk;
///
/// let awk = Awk::new("{ print FILENAME }").unwrap();
/// let (output, error) = awk.run(vec!["ignored".into()], Some("data/input.txt".into()), None);
/// assert_eq!(output, vec!["data/input.txt".to_string()]);
/// assert!(error.is_none());
/// ```
///
/// Use a custom field separator to parse CSV-style input:
///
/// ```
/// use rawk_core::awk::Awk;
///
/// let awk = Awk::new("{ print $1 }").unwrap();
/// let (output, error) = awk.run(vec!["Alice,30,engineer".into()], None, Some(",".into()));
/// assert_eq!(output, vec!["Alice".to_string()]);
/// assert!(error.is_none());
/// ```
pub struct Awk {
    program: Program<'static>,
}

impl Awk {
    /// Parse an AWK script into an executable program.
    ///
    /// The script is stored with a static lifetime to keep the AST valid.
    /// Returns a parse error if the script is not valid AWK according to this parser.
    pub fn new(script: impl Into<String>) -> Result<Self, ParseError<'static>> {
        let script: String = script.into();
        let script: &'static str = Box::leak(script.into_boxed_str());

        let lexer = Lexer::new(script);
        let parser: &'static mut Parser<'static> = Box::leak(Box::new(Parser::new(lexer)));
        let program = parser.try_parse_program()?;

        Ok(Self { program })
    }

    /// Execute the compiled program against the given input lines.
    ///
    /// - `filename` — exposed as the `FILENAME` built-in variable inside the script.
    ///   Pass `None` to use the default value `"-"` (conventional stdin placeholder).
    /// - `field_separator` — overrides the `FS` built-in variable used to split each
    ///   input record into fields (`$1`, `$2`, …). Pass `None` to use the default `" "`,
    ///   which splits on runs of whitespace.
    ///
    /// Returns `(output_lines, runtime_error)`. When a runtime error occurs, output
    /// collected before the error is still returned alongside the error message.
    pub fn run(
        &self,
        input: impl IntoIterator<Item = String> + 'static,
        filename: Option<String>,
        field_separator: Option<String>,
    ) -> (Vec<String>, Option<String>) {
        let filename = filename.unwrap_or_else(|| "-".to_string());
        let mut evaluator = Evaluator::new(self.program.clone(), input, filename);
        if let Some(fs) = field_separator {
            evaluator = evaluator.with_field_separator(fs);
        }

        let output = evaluator.eval();
        (output, evaluator.runtime_error().map(str::to_string))
    }

    /// Like [`run`](Self::run), but writes output directly to the given writer
    /// instead of collecting it. This avoids buffering all output in memory.
    pub fn run_to_writer(
        &self,
        input: impl IntoIterator<Item = String> + 'static,
        filename: Option<String>,
        field_separator: Option<String>,
        writer: Box<dyn Write>,
    ) -> Option<String> {
        let filename = filename.unwrap_or_else(|| "-".to_string());
        let mut evaluator = Evaluator::new(self.program.clone(), input, filename)
            .with_writer(writer);
        if let Some(fs) = field_separator {
            evaluator = evaluator.with_field_separator(fs);
        }

        evaluator.eval();
        evaluator.runtime_error().map(str::to_string)
    }
}
