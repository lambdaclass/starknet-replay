use std::fs::File;

use cairo_lang_sierra::program::Program;
use cairo_native::{Value, context::NativeContext, executor::AotNativeExecutor};

fn main() {
    let mut args = std::env::args();
    args.next();

    let sierra_path = args.next().expect("expected cairo path as first argument");
    let sierra_file = File::open(sierra_path).expect("failed to open sierra file");

    let sierra_program: Program =
        serde_json::from_reader(sierra_file).expect("failed to deserialize program");

    let native_context = NativeContext::new();
    let native_module = native_context
        .compile(&sierra_program, false, Some(Default::default()))
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
