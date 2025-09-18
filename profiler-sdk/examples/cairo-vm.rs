use std::{collections::HashMap, fs::File};

use itertools::Itertools;
use profiler_sdk::{
    schema::{IndexIntoResourceTable, Profile},
    transforms::{
        collapse_recursion, collapse_resource, collapse_subtree, focus_on_function, merge_function,
        rename_function,
    },
    tree::Tree,
};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("expected profile path as first argument");
    let file = File::open(path).expect("failed to open profile");
    let mut profile: Profile =
        serde_json::from_reader(file).expect("failed to deserialize profile");

    {
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
    }

    // Focus on execute
    {
        let func = profile.threads[0]
            .func_table
            .name
            .iter()
            .position(|&name_idx| {
                &profile.shared.string_array[name_idx]
                    == "blockifier::execution::execution_utils::execute_entry_point_call_wrapper"
            })
            .expect("failed to find func");
        focus_on_function(&mut profile, 0, func);
        collapse_recursion(&mut profile, 0, func);
    }

    // Collapse functions
    {
        let funcs = profile.threads[0]
            .func_table
            .name
            .iter()
            .positions(|&name_idx| {
                let name = &profile.shared.string_array[name_idx];
                name == "<blockifier::execution::deprecated_syscalls::hint_processor::DeprecatedSyscallHintProcessor as cairo_vm::hint_processor::hint_processor_definition::HintProcessorLogic>::execute_hint" ||
                name == "blockifier::execution::deprecated_entry_point_execution::finalize_execution" || 
                name == "blockifier::execution::deprecated_entry_point_execution::initialize_execution_context" || 
                name == "blockifier::execution::deprecated_entry_point_execution::prepare_call_arguments" || 
                name == "cairo_vm::vm::runners::cairo_runner::CairoRunner::get_hint_data" ||
                name == "cairo_vm::vm::runners::cairo_runner::CairoRunner::initialize_function_entrypoint" ||
                name == "cairo_vm::vm::runners::cairo_runner::CairoRunner::initialize_vm" ||
                name == "cairo_vm::vm::security::verify_secure_runner" ||
                name == "cairo_vm::vm::vm_core::VirtualMachine::step_instruction" ||
                name == "cairo_vm::vm::vm_core::VirtualMachine::verify_auto_deductions" ||
                name == "cairo_vm::vm::vm_memory::memory_segments::MemorySegmentManager::gen_cairo_arg" ||
                name == "core::ptr::drop_in_place<cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::HintProcessorData>"
            })
            .collect_vec();

        for func in funcs {
            collapse_subtree(&mut profile, 0, func);
        }
    }

    // Merge functions
    {
        let funcs = profile.threads[0]
            .func_table
            .name
            .iter()
            .positions(|&name_idx| {
                let name = &profile.shared.string_array[name_idx];
                name == "blockifier::execution::execution_utils::execute_entry_point_call" ||
                    name == "blockifier::execution::deprecated_entry_point_execution::execute_entry_point_call" ||
                    name == "blockifier::execution::deprecated_entry_point_execution::run_entry_point" ||
                    name == "<blockifier::execution::deprecated_syscalls::hint_processor::DeprecatedSyscallHintProcessor as cairo_vm::vm::runners::cairo_runner::ResourceTracker>::consume_step" ||
                    name == "<blockifier::execution::deprecated_syscalls::hint_processor::DeprecatedSyscallHintProcessor as cairo_vm::vm::runners::cairo_runner::ResourceTracker>::consumed" ||
                    name == "<cairo_vm::vm::runners::cairo_runner::RunResources as cairo_vm::vm::runners::cairo_runner::ResourceTracker>::consume_step" ||
                    name == "cairo_vm::vm::runners::cairo_runner::CairoRunner::initialize_vm" ||
                    name == "cairo_vm::vm::security::verify_secure_runner" ||
                    name == "cairo_vm::vm::vm_core::VirtualMachine::verify_auto_deductions" ||
                    name == "cairo_vm::vm::vm_memory::memory_segments::MemorySegmentManager::gen_cairo_arg" ||
                    name == "starknet_types_core::felt::primitive_conversions::<impl core::convert::From<u128> for starknet_types_core::felt::Felt>::from" ||
                    name == "libsystem_c.dylib" ||
                    name == "libsystem_kernel.dylib" ||
                    name == "libsystem_malloc.dylib" ||
                    name == "libsystem_platform.dylib" ||
                    name == "std::sys::pal::unix::time::Timespec::now" ||
                    name == "std::time::Instant::elapsed" ||
                    name.starts_with("__rustc") || 
                    name.starts_with("<alloc") ||
                    name.starts_with("<unknown") ||
                    name.starts_with("alloc") 
            })
            .collect_vec();
        for func in funcs {
            merge_function(&mut profile, 0, func);
        }
    }

    // Rename functions
    {
        let renames = HashMap::from([
            (
                "<blockifier::execution::deprecated_syscalls::hint_processor::DeprecatedSyscallHintProcessor as cairo_vm::hint_processor::hint_processor_definition::HintProcessorLogic>::execute_hint",
                "blockifier::execute_hint",
            ),
            (
                "blockifier::execution::deprecated_entry_point_execution::finalize_execution",
                "blockifier::finalize_execution",
            ),
            (
                "blockifier::execution::deprecated_entry_point_execution::initialize_execution_context",
                "blockifier::initialize_execution_context",
            ),
            (
                "blockifier::execution::deprecated_entry_point_execution::prepare_call_arguments",
                "blockifier::prepare_call_arguments",
            ),
            (
                "blockifier::execution::execution_utils::execute_entry_point_call_wrapper",
                "blockifier::execute_entry_point_call_wrapper",
            ),
            (
                "cairo_vm::vm::runners::cairo_runner::CairoRunner::get_hint_data",
                "cairo_vm::get_hint_data",
            ),
            (
                "cairo_vm::vm::runners::cairo_runner::CairoRunner::initialize_function_entrypoint",
                "cairo_vm::initialize_function_entrypoint",
            ),
            (
                "cairo_vm::vm::runners::cairo_runner::CairoRunner::run_from_entrypoint",
                "cairo_vm::run_from_entrypoint",
            ),
            (
                "cairo_vm::vm::runners::cairo_runner::CairoRunner::run_until_pc",
                "cairo_vm::un_until_pc",
            ),
            (
                "cairo_vm::vm::vm_core::VirtualMachine::step",
                "cairo_vm::step",
            ),
            (
                "cairo_vm::vm::vm_core::VirtualMachine::step_instruction",
                "cairo_vm::step_instruction",
            ),
            (
                "core::ptr::drop_in_place<cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::HintProcessorData>",
                "drop_in_place<cairo_vm::HintProcessorData>",
            ),
        ]);
        let funcs_and_names = profile.threads[0]
            .func_table
            .name
            .iter()
            .enumerate()
            .filter_map(|(func_idx, &name_idx)| {
                let name = profile.shared.string_array[name_idx].as_str();
                let new_name = renames.get(name)?.to_string();
                Some((func_idx, new_name))
            })
            .collect_vec();

        for (func_idx, new_name) in funcs_and_names {
            rename_function(&mut profile, 0, func_idx, new_name);
        }
    }

    println!("{}", Tree::from_profile(&profile, 0));
}
