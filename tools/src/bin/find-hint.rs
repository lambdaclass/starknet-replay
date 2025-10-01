use std::{collections::HashMap, fs::File, process::ExitCode};

use cairo_vm::serde::deserialize_program::HintParams;
use clap::{Parser, command};
use starknet_core::types::ContractClass;
use state_reader::class_manager::decompress_v0_class;

/// Looks for a specific hint code in the given contract class.
///
/// If the hint is found, exits with code 0.
/// Otherwise, exits with code 1.
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    /// Path to contract class.
    class_path: String,
    /// Hint code to find
    hint: String,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let file = File::open(args.class_path).expect("failed to open file");
    let class: ContractClass = serde_json::from_reader(file).expect("failed to read class");

    let ContractClass::Legacy(class) = class else {
        return ExitCode::FAILURE;
    };

    let class = decompress_v0_class(class).expect("failed to decompress class");

    let hints: HashMap<usize, Vec<HintParams>> =
        serde_json::from_value(class.program.hints).expect("failed to read hints");

    for hint in hints.iter().flat_map(|(_, hints)| hints) {
        if hint.code == args.hint {
            return ExitCode::SUCCESS;
        }
    }

    ExitCode::FAILURE
}
