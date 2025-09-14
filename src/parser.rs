use ruff_python_ast::{Expr, Mod, PythonVersion, Stmt};
use ruff_python_parser::{Mode, ParseError, ParseErrorType, ParseOptions, Parsed, parse};
use ruff_text_size::{TextRange, TextSize};
use std::collections::HashSet;
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

pub(crate) fn get_exposed_names(parsed: &Parsed<Mod>, respect_all: bool) -> Vec<String> {
    let mut result = vec![];

    let module_body = match parsed.syntax().as_module() {
        Some(module) => &module.body,
        None => return result,
    };

    // Collect existing __all__, if any.
    let mut all: Option<HashSet<String>> = None;
    if respect_all {
        for stmt in module_body {
            match stmt {
                Stmt::Assign(assign) => match &assign.targets[0] {
                    Expr::Name(name) => {
                        if name.id == "__all__" {
                            let names_iter = match &*assign.value {
                                Expr::List(names) => names
                                    .elts
                                    .iter()
                                    .filter_map(|n| n.as_string_literal_expr())
                                    .map(|n| n.value.to_string()),
                                _ => break,
                            };
                            all = Some(HashSet::from_iter(names_iter));
                            break;
                        }
                    }
                    _ => (),
                },
                _ => (),
            }
        }
    }

    // Helper closure to determine whether a name would normally
    // be exposed in the module.
    let normally_exposed = |name: &String| {
        if name.starts_with("_") {
            return false;
        }

        true
    };

    // Helper closure to add names to the result based on __all__,
    // and whether the name is valid to include.
    let mut push_name = |name: String| match &all {
        None => {
            if normally_exposed(&name) {
                result.push(name)
            }
        }
        Some(all) => {
            if all.contains(&name) {
                result.push(name);
            }
        }
    };

    // Determine top-level names from parsed module.
    for stmt in module_body {
        match stmt {
            // Parse functions.
            Stmt::FunctionDef(def) => {
                push_name(def.name.to_string());
            }
            // Parse classes.
            Stmt::ClassDef(def) => {
                push_name(def.name.to_string());
            }
            // Parse top-level assign statements.
            Stmt::Assign(assign) => {
                if assign.targets.len() == 0 {
                    continue;
                }

                match &assign.targets[0] {
                    // Parse bare assigns (e.g., VAL = "CONSTANT").
                    Expr::Name(name) => {
                        push_name(name.id.to_string());
                    }
                    // Parse tuple assigns (e.g., A, B = "C1", "C2").
                    Expr::Tuple(tuple) => {
                        for expr in &tuple.elts {
                            match expr {
                                Expr::Name(name) => {
                                    push_name(name.id.to_string());
                                }
                                _ => (),
                            }
                        }
                    }
                    _ => (),
                }
            }
            _ => (),
        }
    }

    result
}
