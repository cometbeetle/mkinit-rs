use crate::parser;
use ruff_python_ast::PythonVersion;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use walkdir::WalkDir;

#[derive(Debug)]
pub(crate) enum ImportStatements {
    Import(String),
    FromImport(String, String),
}

use ImportStatements::*;

#[derive(Debug, Clone)]
pub(crate) struct InitMaker {
    root: PathBuf,
    py_version: PythonVersion,
    respect_all: bool,
    sort_imports: bool,
    verbose: bool,
}

impl InitMaker {
    pub(crate) fn new(
        root: PathBuf,
        py_version: PythonVersion,
        respect_all: bool,
        sort_imports: bool,
        verbose: bool,
    ) -> Self {
        Self {
            root,
            py_version,
            respect_all,
            sort_imports,
            verbose,
        }
    }

    pub(crate) fn with_root(&self, root: PathBuf) -> Self {
        let mut new = self.clone();
        new.root = root;
        new
    }

    pub(crate) fn make_init(&self) -> Result<Vec<ImportStatements>, String> {
        if !self.root.exists() {
            return Err(format!("Path {} does not exist", self.root.display()));
        }

        if self.root.is_dir() {
            let mut result = vec![];

            for submod in WalkDir::new(&self.root)
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

                let submod_init = self.with_root(submod.path().to_path_buf()).make_init()?;

                // Don't expose directories with no modules.
                if submod.path().is_dir() && submod_init.is_empty() {
                    continue;
                }

                // Import the module itself.
                result.push(Import(submod_name.to_string()));

                // Import each exposed attribute.
                for statement in submod_init {
                    match statement {
                        Import(item) => result.push(FromImport(submod_name.to_string(), item)),
                        FromImport(_, attr) => {
                            result.push(FromImport(submod_name.to_string(), attr))
                        }
                    }
                }
            }

            // Create or modify the __init__.py file for the directory.
            match self.process_init_file(&result) {
                Ok(_) => Ok(result),
                Err(e) => Err(e),
            }
        } else if self.root.is_file() && self.root.extension() == Some("py".as_ref()) {
            let parsed = match parser::parse_module(&self.root, self.py_version) {
                Ok(ast) => ast,
                Err(e) => return Err(e.to_string()),
            };

            Ok(Vec::from_iter(
                parser::get_exposed_names(&parsed, self.respect_all)
                    .into_iter()
                    .map(|a| Import(a)),
            ))
        } else {
            Err(format!(
                "Path {} is not a directory or Python source file",
                self.root.display()
            ))
        }
    }

    pub(crate) fn process_init_file(
        &self,
        statements: &Vec<ImportStatements>,
    ) -> Result<(), String> {
        // Don't process the __init__.py file if nothing is to be exposed.
        if statements.is_empty() {
            return Ok(());
        }

        let init_path = self.root.join("__init__.py");
        let existing_contents = fs::read_to_string(&init_path).unwrap_or_else(|_| String::new());
        let mut new_contents = String::new();

        // TODO: Process existing contents in some way.

        // TODO: Need to also determine how to handle protected / private things
        //       in the generated init files; will involve parsing __protected__, __private__, etc.
        //       (see the current mkinit implementation)

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

        if self.sort_imports {
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
        if self.sort_imports {
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

        match fs::write(&init_path, new_contents) {
            Ok(_) => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}
