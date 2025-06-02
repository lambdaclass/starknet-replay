use cairo_lang_compiler::{diagnostics::DiagnosticsReporter, CompilerConfig};
use cairo_lang_starknet::compile::starknet_compile;
use clap::{command, Parser};

/// Compiles a Cairo contract
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // Path to cairo file
    cairo_path: String,
}

fn main() {
    let args = Args::parse();

    let diagnostics_reporter = DiagnosticsReporter::stderr().allow_warnings();

    let contract = starknet_compile(
        args.cairo_path.into(),
        None,
        Some(CompilerConfig {
            replace_ids: true,
            diagnostics_reporter,
            ..CompilerConfig::default()
        }),
        None,
    )
    .expect("failed to compile sierra");

    print!("{}", contract);
}
