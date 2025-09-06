mod codegen;
mod parser;

use clap::Parser;
use ruff_python_ast::PythonVersion;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    dir: String,

    #[arg(short, long, default_value = "3.12")]
    python_version: String,

    #[arg(long, default_value = "true")]
    respect_all: bool,

    #[arg(short, long, default_value = "true")]
    sort_imports: bool,

    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();

    let python_version = match PythonVersion::from_str(args.python_version.as_str()) {
        Ok(v) => v,
        Err(_) => {
            eprintln!("Python version {} is not valid", args.python_version);
            return;
        }
    };

    match codegen::process_project(
        &PathBuf::from(args.dir),
        python_version,
        args.respect_all,
        args.sort_imports,
        args.verbose,
    ) {
        Ok(_) => {}
        Err(error) => eprintln!("{}", error),
    }

    //let _ = codegen::process_project(&PathBuf::from("./examples"), PythonVersion::PY312);
}
