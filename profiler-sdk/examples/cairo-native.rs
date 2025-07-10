use std::fs::File;

use itertools::Itertools;
use profiler_sdk::{
    schema::{IndexIntoResourceTable, Profile},
    transforms::{
        collapse_frames, collapse_recursion, collapse_resource, collapse_subtree, drop_function,
        focus_on_function, merge_function,
    },
    tree::Tree,
};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("expected profile path as first argument");
    let file = File::open(path).expect("failed to open profile");
    let profile: Profile = serde_json::from_reader(file).expect("failed to deserialize profile");

    {
        println!("│ GROUP BY SYMBOL");
        println!("│ ---------------");

        let mut profile = profile.clone();

        // Find main resource.
        let main_resource = {
            let main_function = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| profile.shared.string_array[name_idx] == "main")
                .expect("main function should exist");
            let main_resource: Option<IndexIntoResourceTable> =
                profile.threads[0].func_table.resource[main_function].into();
            main_resource.expect("main resource should exist")
        };

        // Collapse all resources except main resource.
        for resource_idx in 0..profile.threads[0].resource_table.length {
            if resource_idx != main_resource {
                collapse_resource(&mut profile, 0, resource_idx);
            }
        }

        // Merge unimportant functions
        {
            collapse_frames(&mut profile, 0, "utils".to_string(), |frame| {
                let name = frame.func().name();
                name.contains("<unknown")
                    || name.contains("as core::")
                    || name.starts_with("<core::")
                    || name.starts_with("core::")
                    || name.contains("alloc::")
                    || name.contains("ark_ec::")
                    || name.contains("ark_ff::")
                    || name.contains("ark_secp256r1::")
                    || name.contains("ark_serialize::")
                    || name.contains("block_buffer::")
                    || name.contains("digest::")
                    || name.contains("generic_array::")
                    || name.contains("hashbrown::")
                    || name.contains("hex::")
                    || name.contains("keccak::")
                    || name.contains("lambdaworks")
                    || name.contains("log::")
                    || name.contains("num_bigint::")
                    || name.contains("num_integer::")
                    || name.contains("num_rational::")
                    || name.contains("rand::")
                    || name.contains("serde_json::")
                    || name.contains("serde::")
                    || name.contains("sha2::")
                    || name.contains("sha3::")
                    || name.contains("smallvec::")
                    || name.contains("starknet_api::")
                    || name.contains("starknet_types_core::")
                    || name.contains("std::")
                    || name.contains("tracing_subscriber::")
                    || name.contains("tracing_core::")
                    || name.contains("sem_ver::")
                    || name == "__rdl_alloc"
                    || name == "__rdl_realloc"
                    || name == "__rdl_dealloc"
                    || name == "__rust_alloc"
                    || name == "__rust_dealloc"
                    || name == "__rust_realloc"
                    || name == "_rdl_alloc"
                    || name == "_rdl_dealloc"
                    || name == "_rdl_realloc"
                    || name == "libcompiler_rt.dylib"
                    || name == "libdyld.dylib"
                    || name == "libsystem_c.dylib"
                    || name == "libsystem_kernel.dylib"
                    || name == "libsystem_malloc.dylib"
                    || name == "libsystem_platform.dylib"
                    || name == "invoke_trampoline"
            });
            let funcs = profile.threads[0]
                .func_table
                .name
                .iter()
                .positions(|&name_idx| &profile.shared.string_array[name_idx] == "utils")
                .collect_vec();
            for func in funcs {
                merge_function(&mut profile, 0, func);
            }
        }

        // Focus on execute
        {
            let func = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| {
                    &profile.shared.string_array[name_idx]
                        == "blockifier::transaction::transactions::ExecutableTransaction::execute"
                })
                .expect("failed to find func");
            focus_on_function(&mut profile, 0, func);
        }

        // Drop replay
        {
            collapse_frames(&mut profile, 0, "replay::".to_string(), |frame| {
                let name = frame.func().name();
                name.starts_with("rpc_state_reader")
                    || name.starts_with("<rpc_state_reader")
                    || name.starts_with("<replay::")
            });
            let func = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| {
                    let name = &profile.shared.string_array[name_idx];
                    name == "replay::"
                })
                .expect("failed to find function");

            drop_function(&mut profile, 0, func);
        }

        // Collapse all contract shared libraries into a single frame.
        {
            collapse_frames(&mut profile, 0, "sierra".to_string(), |frame| {
                let Some(resource_idx) = frame.func().resource_idx() else {
                    return false;
                };
                let resource_name_idx = frame.thread.resource_table.name[resource_idx];
                frame.profile.shared.string_array[resource_name_idx].starts_with("0x")
            });
        }

        // Collapse cairo_vm
        {
            collapse_frames(&mut profile, 0, "cairo_vm::".to_string(), |frame| {
                let name = frame.func().name();
                name.starts_with("cairo_vm::")
                    || name.starts_with("<cairo_vm::")
                    || name.starts_with("<&cairo_vm::")
                    || name.starts_with("<cairo_lang_casm::")
            });
            let func = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| profile.shared.string_array[name_idx] == "cairo_vm::")
                .expect("failed to find cairo_vm function");
            collapse_subtree(&mut profile, 0, func);
        }

        // Collapse cairo_native_run
        {
            let funcs = profile.threads[0]
                .func_table
                .name
                .iter()
                .positions(|&name_idx| {
                    let name = &profile.shared.string_array[name_idx];
                    name == "cairo_native::executor::contract::AotContractExecutor::run"
                })
                .collect_vec();

            for func in funcs {
                collapse_subtree(&mut profile, 0, func);
            }
        }

        // Collapse and focus blockifier
        {
            collapse_frames(&mut profile, 0, "blockifier::".to_string(), |frame| {
                let name = frame.func().name();
                name.starts_with("blockifier::")
                    || name.starts_with("<blockifier::")
                    || name.starts_with("<&mut blockifier::")
            });
            let func = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| &profile.shared.string_array[name_idx] == "blockifier::")
                .expect("failed to find func");
            focus_on_function(&mut profile, 0, func);
            collapse_recursion(&mut profile, 0, func);
        }

        println!("{}", Tree::from_profile(&profile, 0));
    }
}
