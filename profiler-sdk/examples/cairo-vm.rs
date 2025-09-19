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
                name == "cairo_vm::vm::vm_core::VirtualMachine::deduce_memory_cell"
                    || name == "cairo_vm::vm::vm_core::VirtualMachine::insert_deduced_operands"
                    || name == "blockifier::execution::deprecated_syscalls::deprecated_syscall_executor::execute_next_deprecated_syscall"
                    || name == "<cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::BuiltinHintProcessor as cairo_vm::hint_processor::hint_processor_definition::HintProcessorLogic>::execute_hint"
                    || name == "cairo_vm::vm::runners::cairo_runner::CairoRunner::get_hint_data"
                    || name == "core::ptr::drop_in_place<alloc::vec::Vec<alloc::boxed::Box<dyn core::any::Any>>>"
                    || name == "cairo_vm::vm::runners::cairo_runner::CairoRunner::initialize_function_entrypoint"
                    || name == "core::ptr::drop_in_place<cairo_vm::vm::runners::cairo_runner::CairoRunner>"
                    || name == "cairo_vm::vm::runners::cairo_runner::CairoRunner::new_v2"
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
                name == "blockifier::execution::execution_utils::execute_entry_point_call"
                    || name == "blockifier::execution::deprecated_entry_point_execution::execute_entry_point_call"
                    || name == "blockifier::execution::deprecated_entry_point_execution::run_entry_point"
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
                "blockifier::DeprecatedSyscallHintProcessor::execute_hint",
            ),
            (
                "blockifier::execution::deprecated_syscalls::deprecated_syscall_executor::execute_next_deprecated_syscall",
                "blockifier::execute_next_deprecated_syscall",
            ),
            (
                "<cairo_vm::hint_processor::builtin_hint_processor::builtin_hint_processor_definition::BuiltinHintProcessor as cairo_vm::hint_processor::hint_processor_definition::HintProcessorLogic>::execute_hint",
                "cairo_vm::BuiltinHintProcessor::execute_hint",
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

    let mut tree = Tree::from_profile(&profile, 0);
    tree.prune(0.01);
    println!("{}", tree);
}
