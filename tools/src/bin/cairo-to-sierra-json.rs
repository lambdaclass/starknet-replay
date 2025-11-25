use std::{io::stdout, path::PathBuf};

use cairo_lang_compiler::{
    CompilerConfig, compile_prepared_db, db::RootDatabase, project::setup_project,
};
use cairo_lang_filesystem::ids::CrateInput;
use clap::Parser;

/// Compiles a Cairo program to sierra json
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // Path to cairo program
    cairo_path: PathBuf,
}

fn main() {
    let args = Args::parse();

    let mut db = RootDatabase::builder()
        .detect_corelib()
        .build()
        .expect("failed to build database");
    let main_crate_ids = {
        let main_crate_inputs = setup_project(&mut db, &args.cairo_path).expect("failed to setup project");
        CrateInput::into_crate_ids(&db, main_crate_inputs)
    };

    let program = compile_prepared_db(
        &db,
        main_crate_ids,
        CompilerConfig {
            replace_ids: true,
            ..Default::default()
        },
    )
    .expect("failed to compile cairo")
    .program;

    serde_json::to_writer_pretty(stdout(), &program).expect("failed to serialize program");
}
