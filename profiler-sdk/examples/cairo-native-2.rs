use std::{
    collections::{HashMap, HashSet},
    fs::File,
};

use itertools::Itertools;
use profiler_sdk::{
    model::Frame,
    schema::{
        IndexIntoFuncTable, IndexIntoResourceTable, IndexIntoStackTable, Profile, RawSamplesTable,
        RawStackTable,
    },
    tree::Tree,
};

fn collapse_frames<P>(profile: &mut Profile, thread_idx: usize, name: String, mut predicate: P)
where
    P: FnMut(Frame) -> bool,
{
    let name_idx = profile.threads[thread_idx].string_array.len();
    profile.threads[thread_idx].string_array.push(name);

    let mut group_frame = None;

    // We will build a new stack table, with the transformation applied.
    let mut new_stack_table: RawStackTable = RawStackTable::default();
    // Maps old stack table indices to new stack table indices, for updating the other tables.
    let mut old_stack_to_new_stack = HashMap::<IndexIntoStackTable, IndexIntoStackTable>::new();
    // Lists stacks that we have already collapsed.
    let mut collapsed_stacks: HashSet<IndexIntoStackTable> = HashSet::new();

    let stack_table_length = profile.threads[thread_idx].stack_table.length;
    for stack in 0..stack_table_length {
        let frame_idx = profile.threads[thread_idx].stack_table.frame[stack];
        let prefix_idx = profile.threads[thread_idx].stack_table.prefix[stack];
        let func_idx = profile.threads[thread_idx].frame_table.func[frame_idx];
        let resource_idx = profile.threads[thread_idx].func_table.resource[func_idx];

        let new_prefix_idx = prefix_idx.map(|prefix_idx| {
            *old_stack_to_new_stack
                .get(&prefix_idx)
                .expect("all previous stack should have been mapped already")
        });

        // Check if the frame should be collapsed.
        if predicate(Frame::new(profile, &profile.threads[thread_idx], frame_idx)) {
            let thread = &mut profile.threads[thread_idx];

            // If its the first time, we build a func and frame for the collapsed frames.
            let group_frame = *group_frame.get_or_insert_with(|| {
                thread.func_table.length += 1;
                thread.func_table.name.push(name_idx);
                thread.func_table.resource.push(resource_idx);
                thread.func_table.file_name.push(None);
                thread.func_table.line_number.push(None);
                thread.func_table.column_number.push(None);
                thread.func_table.is_js.push(false);
                thread.func_table.relevant_for_js.push(false);
                let group_func = thread.func_table.length - 1;
                thread.frame_table.func[frame_idx] = group_func;
                frame_idx
            });

            // Check if the parent frame has been colapsed.
            if new_prefix_idx.is_some_and(|new_prefix| collapsed_stacks.contains(&new_prefix)) {
                // Just reuse that parent frame stack.
                old_stack_to_new_stack.insert(stack, new_prefix_idx.expect("we just checked"));
            } else {
                // If the prefix has not been colapsed, we push an entry for the collapsed stack
                new_stack_table.length += 1;
                new_stack_table.frame.push(group_frame);
                new_stack_table.prefix.push(new_prefix_idx);
                let new_stack = new_stack_table.length - 1;
                old_stack_to_new_stack.insert(stack, new_stack);
                collapsed_stacks.insert(new_stack);
            }
        } else {
            // If the current frame should not be colapsed, we the copy the entry over.
            new_stack_table.length += 1;
            new_stack_table.frame.push(frame_idx);
            new_stack_table.prefix.push(new_prefix_idx);
            let new_stack = new_stack_table.length - 1;
            old_stack_to_new_stack.insert(stack, new_stack);
        }
    }

    let thread = &mut profile.threads[thread_idx];
    thread.stack_table = new_stack_table;
    for stack in &mut thread.samples.stack {
        *stack = *old_stack_to_new_stack
            .get(stack)
            .expect("all stack entries should be mapped");
    }
}

fn focus_on_function(
    profile: &mut Profile,
    thread_idx: usize,
    func_to_collapse: IndexIntoFuncTable,
) {
    // We will build a new stack table, with the transformation applied.
    let mut new_stack_table: RawStackTable = RawStackTable::default();
    // Maps old stack table indices to new stack table indices, for updating the other tables.
    // If a stack index is missing, the sample containing it should be removed.
    let mut old_stack_to_new_stack = HashMap::<IndexIntoStackTable, IndexIntoStackTable>::new();

    let stack_table_length = profile.threads[thread_idx].stack_table.length;
    for stack_idx in 0..stack_table_length {
        let frame_idx = profile.threads[thread_idx].stack_table.frame[stack_idx];
        let prefix_idx = profile.threads[thread_idx].stack_table.prefix[stack_idx];
        let func_idx = profile.threads[thread_idx].frame_table.func[frame_idx];

        let new_prefix =
            prefix_idx.and_then(|prefix_idx| old_stack_to_new_stack.get(&prefix_idx).cloned());
        if new_prefix.is_some() || func_idx == func_to_collapse {
            let new_stack_idx = new_stack_table.length;

            new_stack_table.length += 1;
            new_stack_table.frame.push(frame_idx);
            new_stack_table.prefix.push(new_prefix);

            old_stack_to_new_stack.insert(stack_idx, new_stack_idx);
        }
    }

    let thread = &mut profile.threads[thread_idx];

    // We will build a new sample table, with the transformation applied and
    // without the samples outside of the focused function.
    let mut new_sample_table: RawSamplesTable = RawSamplesTable::default();

    for sample in 0..thread.samples.length {
        let stack = thread.samples.stack[sample];
        if let Some(new_stack) = old_stack_to_new_stack.get(&stack).cloned() {
            new_sample_table.length += 1;
            new_sample_table.stack.push(new_stack);
            new_sample_table.time.push(thread.samples.time[sample]);
            new_sample_table.weight.push(thread.samples.weight[sample]);
            new_sample_table
                .thread_cpu_delta
                .push(thread.samples.thread_cpu_delta[sample]);
        }
    }

    thread.stack_table = new_stack_table;
    thread.samples = new_sample_table;
}

fn collapse_resource(
    profile: &mut Profile,
    thread_idx: usize,
    resource_to_collapse: IndexIntoResourceTable,
) {
    let name_idx = profile.threads[thread_idx].resource_table.name[resource_to_collapse];
    let name = profile.threads[thread_idx].string_array[name_idx].clone();

    collapse_frames(profile, thread_idx, name, |frame| {
        frame.func().resource_idx() == resource_to_collapse
    });
}

fn collapse_subtree(
    profile: &mut Profile,
    thread_idx: usize,
    func_to_collapse: IndexIntoFuncTable,
) {
    // Maps old stack table indices to new stack table indices, for updating the other tables.
    let mut old_stack_to_new_stack = HashMap::<IndexIntoStackTable, IndexIntoStackTable>::new();
    // Determines if a subtree should be collapsed.
    let mut is_in_collapsed_subtree = HashSet::<IndexIntoStackTable>::new();

    let thread = &mut profile.threads[thread_idx];
    for stack in 0..thread.stack_table.length {
        let frame = thread.stack_table.frame[stack];
        let func = thread.frame_table.func[frame];
        let prefix = thread.stack_table.prefix[stack];

        if prefix.is_some_and(|prefix| is_in_collapsed_subtree.contains(&prefix)) {
            // If the prefix should be collapsed, we map current stack to prefix stack.
            let prefix = prefix.expect("we just checked that there is a prefix");
            let new_prefix = old_stack_to_new_stack.get(&prefix).cloned().unwrap_or(0);
            old_stack_to_new_stack.insert(stack, new_prefix);
            is_in_collapsed_subtree.insert(stack);
        } else {
            // prefix won't be collapsed, so we keep the current stack entry.
            old_stack_to_new_stack.insert(stack, stack);

            if func == func_to_collapse {
                // if the current function should me collapsed, mark all subtree for collapsing.
                is_in_collapsed_subtree.insert(stack);
            }
        }
    }

    for stack in &mut thread.samples.stack {
        *stack = *old_stack_to_new_stack
            .get(stack)
            .expect("all stack entries should be mapped");
    }
}

fn collapse_recursion(
    profile: &mut Profile,
    thread_idx: usize,
    func_to_collapse: IndexIntoFuncTable,
) {
    let thread = &mut profile.threads[thread_idx];

    let mut stack_to_new_prefix =
        HashMap::<IndexIntoStackTable, Option<IndexIntoStackTable>>::new();

    for stack in 0..thread.stack_table.length {
        let prefix = thread.stack_table.prefix[stack];
        let frame = thread.stack_table.frame[stack];
        let func = thread.frame_table.func[frame];

        // check if our prefix has been mapped.
        // - None: Our prefix is not part of the of the function to collapse.
        // - Some(None): The subtree prefix is the root of the whole tree.
        // - Some(Some(stack_idx)): The subtree prefix is given by the stack index.
        let subtree_prefix = prefix.and_then(|prefix| stack_to_new_prefix.get(&prefix).cloned());

        match subtree_prefix {
            None => {
                if func == func_to_collapse {
                    // if our prefix is not part of the function to colapse, and
                    // the current function should be collapsed, then this node
                    // is the root of the tree.
                    stack_to_new_prefix.insert(stack, prefix);
                }
            }
            Some(subtree_prefix) => {
                // our prefix is part of the subtree of the function to colapse
                stack_to_new_prefix.insert(stack, subtree_prefix);
                if func == func_to_collapse {
                    // if we find a recursive call, reparent the current node to
                    // the root of the tree.
                    thread.stack_table.prefix[stack] = subtree_prefix;
                }
            }
        }
    }
}

fn merge_function(profile: &mut Profile, thread_idx: usize, function_to_merge: IndexIntoFuncTable) {
    // We will build a new stack table, with the transformation applied.
    let mut new_stack_table: RawStackTable = RawStackTable::default();
    // Maps old stack table indices to new stack table indices, for updating the other tables.
    let mut old_stack_to_new_stack = HashMap::<IndexIntoStackTable, IndexIntoStackTable>::new();

    let thread = &profile.threads[thread_idx];
    for stack in 0..thread.stack_table.length {
        let prefix = thread.stack_table.prefix[stack];
        let frame = thread.stack_table.frame[stack];
        let func = thread.frame_table.func[frame];

        let new_prefix = prefix.and_then(|prefix| old_stack_to_new_stack.get(&prefix).cloned());

        if func == function_to_merge {
            if let Some(new_prefix) = new_prefix {
                old_stack_to_new_stack.insert(stack, new_prefix);
            }
        } else {
            old_stack_to_new_stack.insert(stack, stack);
        }

        new_stack_table.length += 1;
        new_stack_table.frame.push(frame);
        new_stack_table.prefix.push(new_prefix);
    }

    let thread = &mut profile.threads[thread_idx];

    // We will build a new sample table, with the transformation applied and
    // without the samples outside of the focused function.
    let mut new_sample_table: RawSamplesTable = RawSamplesTable::default();
    for sample in 0..thread.samples.length {
        let stack = thread.samples.stack[sample];
        if let Some(new_stack) = old_stack_to_new_stack.get(&stack).cloned() {
            new_sample_table.length += 1;
            new_sample_table.stack.push(new_stack);
            new_sample_table.time.push(thread.samples.time[sample]);
            new_sample_table.weight.push(thread.samples.weight[sample]);
            new_sample_table
                .thread_cpu_delta
                .push(thread.samples.thread_cpu_delta[sample]);
        }
    }

    thread.stack_table = new_stack_table;
    thread.samples = new_sample_table;
}

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
        // Collapse recursion for Sierra.
        {
            let func = profile.threads[0]
                .func_table
                .name
                .iter()
                .position(|&name_idx| &profile.threads[0].string_array[name_idx] == "sierra")
                .expect("failed to find func");
            collapse_recursion(&mut profile, 0, func);
        }

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
                    || name.contains("{{closure}}")
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
