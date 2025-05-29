use std::{
    collections::{HashMap, HashSet},
    fs::File,
};

use profiler_sdk::{
    model::Frame,
    schema::{IndexIntoResourceTable, IndexIntoStackTable, Profile, RawStackTable},
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
    // let mut old_prefix_to_new_prefix = HashMap::<IndexIntoStackTable, IndexIntoStackTable>::new();
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

fn merge_funcs(profile: &mut Profile, thread_idx: usize, funcs: &[&str], new_func_name: String) {
    let mut new_func = None;

    let thread = &mut profile.threads[thread_idx];
    thread.string_array.push(new_func_name);
    let new_func_name_idx = thread.string_array.len() - 1;

    for stack in 0..thread.stack_table.length {
        let frame = thread.stack_table.frame[stack];
        let func = thread.frame_table.func[frame];
        let name_idx = thread.func_table.name[func];
        let name = thread.string_array[name_idx].as_str();

        if !funcs.contains(&name) {
            continue;
        }

        match new_func {
            Some(new_func) => thread.frame_table.func[frame] = new_func,
            None => {
                thread.func_table.name[func] = new_func_name_idx;
                new_func = Some(func);
            }
        }
    }
}

fn collapse_recursion(profile: &mut Profile, thread_idx: usize, func_to_collapse: &str) {
    let thread = &mut profile.threads[thread_idx];

    let mut stack_to_new_prefix =
        HashMap::<IndexIntoStackTable, Option<IndexIntoStackTable>>::new();

    for stack in 0..thread.stack_table.length {
        let prefix = thread.stack_table.prefix[stack];
        let frame = thread.stack_table.frame[stack];
        let func = thread.frame_table.func[frame];
        let name_idx = thread.func_table.name[func];
        let name = thread.string_array[name_idx].as_str();

        // check if our prefix has been mapped.
        // - None: Our prefix is not part of the of the function to collapse.
        // - Some(None): The subtree prefix is the root of the whole tree.
        // - Some(Some(stack_idx)): The subtree prefix is given by the stack index.
        let subtree_prefix = prefix.and_then(|prefix| stack_to_new_prefix.get(&prefix).cloned());

        match subtree_prefix {
            None => {
                if name == func_to_collapse {
                    // if our prefix is not part of the function to colapse, and
                    // the current function should be collapsed, then this node
                    // is the root of the tree.
                    stack_to_new_prefix.insert(stack, prefix);
                }
            }
            Some(subtree_prefix) => {
                // our prefix is part of the subtree of the function to colapse
                stack_to_new_prefix.insert(stack, subtree_prefix);
                if name == func_to_collapse {
                    // if we find a recursive call, reparent the current node to
                    // the root of the tree.
                    thread.stack_table.prefix[stack] = subtree_prefix;
                }
            }
        }
    }
}

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("expected profile path as first argument");
    let file = File::open(path).expect("failed to open profile");
    let mut profile: Profile =
        serde_json::from_reader(file).expect("failed to deserialize profile");

    {
        let thread = &profile.threads[0];
        for resource_idx in 0..thread.resource_table.length {
            collapse_resource(&mut profile, 0, resource_idx);
        }
        collapse_recursion(&mut profile, 0, "replay");

        merge_funcs(
            &mut profile,
            0,
            &[
                "0x816dd0297efc55dc1e7559020a3a825e81ef734b558f03c83325d4da7e6253.dylib",
                "0x5dde112c893e2f5ed85b92a08d93cfa5579ce95d27afb34e47b7e7aad59c1c0.dylib",
                "0x4247b4b4eef40ec5d47741f5cc911239c1bbd6768b86c240f4304687f70f017.dylib",
            ],
            "MLIR".to_string(),
        );
        collapse_recursion(&mut profile, 0, "MLIR");

        merge_funcs(
            &mut profile,
            0,
            &[
                "libcompiler_rt.dylib",
                "libdyld.dylib",
                "libLLVM.dylib",
                "libsystem_c.dylib",
                "libsystem_kernel.dylib",
                "libsystem_malloc.dylib",
                "libsystem_platform.dylib",
            ],
            "lib.dylib".to_string(),
        );
        collapse_recursion(&mut profile, 0, "lib.dylib");
    }

    let tree = Tree::from_profile(&profile, 0);
    println!("{}", tree);
}
