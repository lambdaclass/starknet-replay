use std::fs::File;

use cairo_lang_sierra::program::Program;
use cairo_native::{Value, context::NativeContext, executor::AotNativeExecutor};
use clap::{Parser, command};

/// Executes a sierra json program with Cairo Native
#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
struct Args {
    // Path to sierra json file
    sierra_path: String,
}

fn main() {
    let args = Args::parse();

    let sierra_file = File::open(args.sierra_path).expect("failed to open sierra file");

    let sierra_program: Program =
        serde_json::from_reader(sierra_file).expect("failed to deserialize program");

    let native_context = NativeContext::new();
    let native_module = native_context
        .compile(&sierra_program, false, Some(Default::default()), None)
        .expect("failed to compile sierra");

    let executor =
        AotNativeExecutor::from_native_module(native_module, cairo_native::OptLevel::None)
            .expect("failed to create executor");

    let function = sierra_program
        .funcs
        .iter()
        .find(|function| {
            if let Some(debug_name) = &function.id.debug_name {
                debug_name.contains("main")
            } else {
                false
            }
        })
        .expect("failed to find function");

    let result = executor
        .invoke_dynamic(
            &function.id,
            &[Value::Array(vec![]), Value::Array(vec![])],
            Some(u64::MAX),
        )
        .expect("failed to execute function");

    println!(
        "{}",
        serde_json::to_string_pretty(&result).expect("failed to serialize result")
    );
}
