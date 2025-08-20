mod parser;

use clap::Parser;
use ruff_python_ast::PythonVersion;

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    #[arg(short, long)]
    dir: String,

    #[arg(short, long)]
    verbose: bool,
}

fn main() {
    let args = Args::parse();

    let result = parser::parse_dir(args.dir.as_str(), PythonVersion::PY312);

    println!("{:#?}", result);
}
