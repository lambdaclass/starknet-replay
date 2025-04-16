use cairo_lang_compiler::{CompilerConfig, diagnostics::DiagnosticsReporter};
use cairo_lang_starknet::compile::starknet_compile;

fn main() {
    let mut args = std::env::args();
    args.next();

    let cairo_path = args.next().expect("expected cairo path as first argument");

    let diagnostics_reporter = DiagnosticsReporter::stderr().allow_warnings();

    let contract = starknet_compile(
        cairo_path.into(),
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
