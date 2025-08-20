use ruff_python_ast::{Mod, PythonVersion};
use ruff_python_parser::{Mode, ParseError, ParseOptions, Parsed, parse};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

pub(crate) fn parse_dir(
    dir: &str,
    version: PythonVersion,
) -> HashMap<PathBuf, Result<Parsed<Mod>, ParseError>> {
    let mut result = HashMap::new();

    let options = ParseOptions::from(Mode::Module).with_target_version(version);

    for entry in WalkDir::new(dir).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if !entry.file_name().to_string_lossy().ends_with(".py") {
                continue;
            }

            // Read contents of file.
            let file_contents = match fs::read_to_string(entry.path()) {
                Ok(contents) => contents,
                Err(e) => {
                    eprintln!("Error reading file {}: {}", entry.path().display(), e);
                    continue;
                }
            };

            // Parse the file contents.
            result.insert(
                entry.path().to_path_buf(),
                parse(file_contents.as_str(), options.clone()),
            );
        }
    }

    result
}
