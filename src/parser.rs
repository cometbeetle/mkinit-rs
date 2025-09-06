use ruff_python_ast::{Mod, PythonVersion};
use ruff_python_parser::{Mode, ParseError, ParseErrorType, ParseOptions, Parsed, parse};
use ruff_text_size::{TextRange, TextSize};
use std::fs;
use std::path::PathBuf;

pub(crate) fn parse_module(
    file_path: &PathBuf,
    python_version: PythonVersion,
) -> Result<Parsed<Mod>, ParseError> {
    let options = ParseOptions::from(Mode::Module).with_target_version(python_version);

    // Read contents of file.
    let file_contents = match fs::read_to_string(file_path) {
        Ok(contents) => contents,
        Err(e) => {
            return Err(ParseError {
                error: ParseErrorType::OtherError(format!("Failed to read file: {}", e)),
                location: TextRange::empty(TextSize::ZERO),
            });
        }
    };

    parse(file_contents.as_str(), options)
}
