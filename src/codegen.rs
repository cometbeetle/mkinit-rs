use crate::parser;
use ruff_python_ast::{Expr, Mod, PythonVersion, Stmt};
use ruff_python_parser::Parsed;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug)]
enum Imports {
    Import(String),
    FromImport(String, String),
    Attribute(String),
}

use Imports::*;

fn make_init(
    path: &PathBuf,
    python_version: PythonVersion,
    respect_all: bool,
    sort_imports: bool,
    verbose: bool,
) -> Result<Vec<Imports>, String> {
    if !path.exists() {
        return Err(format!("Path {} does not exist", path.display()));
    }

    if path.is_dir() {
        let mut result = vec![];

        for submod in WalkDir::new(path)
            .min_depth(1)
            .max_depth(1)
            .into_iter()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir() || e.path().is_file())
        {
            // Only consider Python source files or directories.
            if submod.path().is_file() && submod.path().extension() != Some("py".as_ref()) {
                continue;
            }

            // __init__.py files are not modules.
            if submod.file_name() == "__init__.py" {
                continue;
            }

            let submod_name = submod.file_name().to_string_lossy();
            let submod_name = submod_name
                .strip_suffix(".py")
                .unwrap_or(submod_name.as_ref());

            let submod_init = make_init(
                &submod.path().to_path_buf(),
                python_version,
                respect_all,
                sort_imports,
                verbose,
            )?;

            // Don't expose directories with no modules.
            if submod.path().is_dir() && submod_init.is_empty() {
                continue;
            }

            // Import the module itself.
            result.push(Import(submod_name.to_string()));

            // Import each exposed attribute.
            for statement in submod_init {
                match statement {
                    Import(module) => result.push(FromImport(submod_name.to_string(), module)),
                    FromImport(_, attr) => result.push(FromImport(submod_name.to_string(), attr)),
                    Attribute(attr) => result.push(FromImport(submod_name.to_string(), attr)),
                }
            }
        }

        // Don't process the __init__.py file if nothing is to be exposed.
        if result.is_empty() {
            return Ok(result);
        }

        // Create or modify the __init__.py file for the directory.
        match process_init_file(
            &path.join("__init__.py"),
            &result,
            python_version,
            sort_imports,
            verbose,
        ) {
            Ok(_) => Ok(result),
            Err(e) => Err(e),
        }
    } else if path.is_file() && path.extension() == Some("py".as_ref()) {
        let parsed = match parser::parse_module(&path, python_version) {
            Ok(ast) => ast,
            Err(e) => return Err(e.to_string()),
        };

        Ok(Vec::from_iter(
            get_exposed_names(&parsed, respect_all)
                .into_iter()
                .map(|a| Attribute(a)),
            // TODO: Need to also determine how to handle protected / private things
            //       in the generated init files; will involve parsing __protected__, __private__, etc.

            // TODO: Basically, need to generalize the system for determining what gets put into
            //       the __init__.py files.

            // TODO: May be good to reorganize, rename functions to better suit this system.
        ))
    } else {
        Err(format!(
            "Path {} is not a directory or Python source file",
            path.display()
        ))
    }
}

fn get_exposed_names(parsed: &Parsed<Mod>, respect_all: bool) -> Vec<String> {
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

fn process_init_file(
    path: &PathBuf,
    statements: &Vec<Imports>,
    python_version: PythonVersion,
    sort_imports: bool,
    verbose: bool,
) -> Result<(), String> {
    let existing_contents = fs::read_to_string(path).unwrap_or_else(|_| String::new());
    let mut new_contents = String::new();

    // TODO: Process existing contents in some way.

    // Collect names to put in the __all__ variable.
    let mut all: Vec<String> = vec![];

    // Collect imports from same source into single vector.
    let mut from_imports_map: HashMap<&String, Vec<&String>> = HashMap::new();

    // Collect each section as vectors.
    let mut imports = Vec::from_iter(statements.iter().filter_map(|s| match s {
        Import(attr) => Some(attr),
        _ => None,
    }));
    let mut from_imports = Vec::from_iter(statements.iter().filter_map(|s| match s {
        FromImport(module, attr) => Some((module, attr)),
        _ => None,
    }));

    if sort_imports {
        imports.sort();
        from_imports.sort();
    }

    // Collect "from .MODULE import ATTR" statements into the HashMap.
    for (module, attr) in from_imports {
        match from_imports_map.get_mut(module) {
            Some(v) => {
                v.push(attr);
            }
            None => {
                from_imports_map.insert(module, vec![attr]);
            }
        }
    }

    // Add "from . import MODULE" lines.
    for module in imports {
        new_contents.push_str(format!("from . import {}\n", module).as_str());
        all.push(module.to_string());
    }
    new_contents.push_str("\n");

    // Add "from .MODULE import ATTR" lines.
    let mut map_keys = Vec::from_iter(from_imports_map.keys());
    if sort_imports {
        map_keys.sort();
    }
    for module in map_keys {
        new_contents.push_str(format!("from .{} import (\n", module).as_str());
        for attr in &from_imports_map[module] {
            new_contents.push_str(format!("    {},\n", attr).as_str());
            all.push(attr.to_string());
        }
        new_contents.push_str(")\n");
    }

    // Add __all__ variable.
    new_contents.push_str("\n__all__ = [\n");
    for name in all {
        new_contents.push_str(format!("    \"{}\",\n", name).as_str());
    }
    new_contents.push_str("]\n");

    match fs::write(path, new_contents) {
        Ok(_) => Ok(()),
        Err(e) => Err(e.to_string()),
    }
}

pub(crate) fn process_project(
    path: &PathBuf,
    python_version: PythonVersion,
    respect_all: bool,
    sort_imports: bool,
    verbose: bool,
) -> Result<(), String> {
    match make_init(path, python_version, respect_all, sort_imports, verbose) {
        Ok(r) => {
            println!("{:?}", r);
            Ok(())
        }
        Err(e) => {
            eprintln!("{}", e);
            Err(e)
        }
    }
}
