//! The transformations algorithms were adapted from firefox profiler source code:
//! - https://github.com/firefox-devtools/profiler/blob/main/src/profile-logic/transforms.js

use std::collections::{HashMap, HashSet};

use crate::{
    model::Frame,
    schema::{
        IndexIntoFuncTable, IndexIntoResourceTable, IndexIntoStackTable, Profile, RawSamplesTable,
        RawStackTable,
    },
};

/// Collapses all frames that match the predicate, into a new function with the given name.
pub fn collapse_frames<P>(profile: &mut Profile, thread_idx: usize, name: String, mut predicate: P)
where
    P: FnMut(Frame) -> bool,
{
    let name_idx = profile.shared.string_array.len();
    profile.shared.string_array.push(name);

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

            // Check if the parent frame has been collapsed.
            if new_prefix_idx.is_some_and(|new_prefix| collapsed_stacks.contains(&new_prefix)) {
                // Just reuse that parent frame stack.
                old_stack_to_new_stack.insert(stack, new_prefix_idx.expect("we just checked"));
            } else {
                // If the prefix has not been collapsed, we push an entry for the collapsed stack
                new_stack_table.length += 1;
                new_stack_table.frame.push(group_frame);
                new_stack_table.prefix.push(new_prefix_idx);
                let new_stack = new_stack_table.length - 1;
                old_stack_to_new_stack.insert(stack, new_stack);
                collapsed_stacks.insert(new_stack);
            }
        } else {
            // If the current frame should not be collapsed, we the copy the entry over.
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

pub fn focus_on_function(
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

pub fn collapse_resource(
    profile: &mut Profile,
    thread_idx: usize,
    resource_to_collapse: IndexIntoResourceTable,
) {
    let name_idx = profile.threads[thread_idx].resource_table.name[resource_to_collapse];
    let name = profile.shared.string_array[name_idx].clone();

    collapse_frames(profile, thread_idx, name, |frame| {
        frame
            .func()
            .resource_idx()
            .is_some_and(|resource_idx| resource_idx == resource_to_collapse)
    });
}

pub fn collapse_subtree(
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

pub fn collapse_recursion(
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
                    // if our prefix is not part of the function to collapse, and
                    // the current function should be collapsed, then this node
                    // is the root of the tree.
                    stack_to_new_prefix.insert(stack, prefix);
                }
            }
            Some(subtree_prefix) => {
                // our prefix is part of the subtree of the function to collapse
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

pub fn merge_function(
    profile: &mut Profile,
    thread_idx: usize,
    function_to_merge: IndexIntoFuncTable,
) {
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
