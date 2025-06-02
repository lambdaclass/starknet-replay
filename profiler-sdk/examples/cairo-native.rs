use std::fs::File;

use itertools::Itertools;
use profiler_sdk::{
    schema::Profile,
    transforms::{
        collapse_frames, collapse_recursion, collapse_resource, collapse_subtree,
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
        println!("│ GROUP BY SHARED LIBRARY");
        println!("│ -----------------------");

        let mut profile = profile.clone();

        let mut mlir_resources = vec![];

        // Collapse all resources except contract shared libraries.
        for resource_idx in 0..profile.threads[0].resource_table.length {
            let name = &profile.threads[0].string_array
                [profile.threads[0].resource_table.name[resource_idx]];
            if name.starts_with("0x") {
                mlir_resources.push(resource_idx);
            } else {
                collapse_resource(&mut profile, 0, resource_idx);
            }
        }
        // Collapse all contract shared libraries into a single function.
        collapse_frames(&mut profile, 0, "MLIR".to_string(), |frame| {
            mlir_resources.contains(&frame.func().resource_idx())
        });
        // Collapse recursion of the replay resource.
        {
            let func = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| &profile.threads[0].string_array[name_idx] == "replay")
                .expect("failed to find func");
            collapse_recursion(&mut profile, 0, func);
        }

        // Focus on the replay resource.
        let replay_function = profile.threads[0]
            .func_table
            .name
            .iter()
            .position(|&name_idx| &profile.threads[0].string_array[name_idx] == "replay");
        if let Some(replay_function) = replay_function {
            focus_on_function(&mut profile, 0, replay_function);
        }

        println!("{}", Tree::from_profile(&profile, 0));
    }

    {
        println!("│ GROUP BY SYMBOL");
        println!("│ ---------------");

        let mut profile = profile.clone();

        // Collapse all resources except contract shared libraries and replay.
        let mut mlir_resources = vec![];
        for resource_idx in 0..profile.threads[0].resource_table.length {
            let name = &profile.threads[0].string_array
                [profile.threads[0].resource_table.name[resource_idx]];

            if name == "replay" {
                continue;
            } else if name.starts_with("0x") {
                mlir_resources.push(resource_idx);
            } else {
                collapse_resource(&mut profile, 0, resource_idx);
            }
        }
        // Collapse all contract shared libraries into a single function.
        collapse_frames(&mut profile, 0, "sierra".to_string(), |frame| {
            mlir_resources.contains(&frame.func().resource_idx())
        });

        // Merge unimportant functions
        {
            collapse_frames(&mut profile, 0, "utils".to_string(), |frame| {
                let name = frame.func().name();
                name.contains("<unknown")
                    || name.contains("alloc::")
                    || name.contains("block_buffer::")
                    || name.contains("cairo_vm::")
                    || name.contains("core::")
                    || name.contains("digest::")
                    || name.contains("hashbrown::")
                    || name.contains("hex::")
                    || name.contains("keccak::")
                    || name.contains("num_bigint::")
                    || name.contains("num_integer::")
                    || name.contains("num_rational::")
                    || name.contains("serde_json::")
                    || name.contains("serde::")
                    || name.contains("sha3::")
                    || name.contains("std::")
                    || name == "__rust_alloc"
                    || name == "__rust_dealloc"
                    || name == "_rdl_alloc"
                    || name == "_rdl_dealloc"
                    || name == "libdyld.dylib"
                    || name == "libsystem_c.dylib"
                    || name == "libsystem_kernel.dylib"
                    || name == "libsystem_malloc.dylib"
                    || name == "libsystem_platform.dylib"
                    || name == "libcompiler_rt.dylib"
                    || name == "invoke_trampoline"
            });
            let funcs = profile.threads[0]
                .func_table
                .name
                .iter()
                .positions(|&name_idx| &profile.threads[0].string_array[name_idx] == "utils")
                .collect_vec();
            for func in funcs {
                merge_function(&mut profile, 0, func);
            }
        }

        // Collapse and focus blockifier
        {
            collapse_frames(&mut profile, 0, "blockifier".to_string(), |frame| {
                let name = frame.func().name();
                name.contains("blockifier") || name.contains("starknet_api")
            });
            let func = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| &profile.threads[0].string_array[name_idx] == "blockifier")
                .expect("failed to find func");
            focus_on_function(&mut profile, 0, func);
            collapse_recursion(&mut profile, 0, func);
        }

        // Collapse math libraries
        {
            collapse_frames(&mut profile, 0, "math".to_string(), |frame| {
                let name = frame.func().name();
                name.contains("starknet_types_core")
                    || name.starts_with("rand")
                    || name.contains("lambdaworks")
            });
            let func = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| {
                    let name = &profile.threads[0].string_array[name_idx];
                    name == "math"
                })
                .expect("failed to find function");
            collapse_subtree(&mut profile, 0, func);
            merge_function(&mut profile, 0, func);
        }

        // Collapse rpc_state_reader crate
        {
            collapse_frames(&mut profile, 0, "rpc_state_reader".to_string(), |frame| {
                frame.func().name().contains("rpc_state_reader")
            });
            let func = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| {
                    let name = &profile.threads[0].string_array[name_idx];
                    name == "rpc_state_reader"
                })
                .expect("failed to find function");
            collapse_subtree(&mut profile, 0, func);
            merge_function(&mut profile, 0, func);
        }

        // Collapse cairo_native::executor::contract::AotContractExecutor::run
        {
            let func = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| {
                    let name = &profile.threads[0].string_array[name_idx];
                    name == "cairo_native::executor::contract::AotContractExecutor::run"
                })
                .expect("failed to find function");

            collapse_subtree(&mut profile, 0, func);
            merge_function(&mut profile, 0, func);
        }

        // Collapse runtime and syscalls
        {
            let funcs = profile.threads[0]
                .func_table
                .name
                .iter()
                .positions(|&name_idx| {
                    let name = &profile.threads[0].string_array[name_idx];
                    name.starts_with("cairo_native::runtime")
                        || name.starts_with("cairo_native::starknet")
                })
                .collect_vec();
            for func in funcs {
                collapse_subtree(&mut profile, 0, func);
            }
        }

        println!("{}", Tree::from_profile(&profile, 0));
    }
}
