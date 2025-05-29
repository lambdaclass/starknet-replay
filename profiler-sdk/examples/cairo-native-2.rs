use std::{
    collections::{HashMap, HashSet},
    fs::File,
};

use profiler_sdk::{
    schema::{IndexIntoResourceTable, IndexIntoStackTable, Profile, RawStackTable},
    tree::Tree,
};

fn collapse_resource(
    profile: &mut Profile,
    thread_idx: usize,
    resource_to_collapse: IndexIntoResourceTable,
) {
    let thread = &mut profile.threads[thread_idx];

    let resource_name_idx = thread.resource_table.name[resource_to_collapse];

    // Build func for resource
    thread.func_table.length += 1;
    thread.func_table.name.push(resource_name_idx);
    thread.func_table.resource.push(resource_to_collapse);
    thread.func_table.file_name.push(None);
    thread.func_table.line_number.push(None);
    thread.func_table.column_number.push(None);
    thread.func_table.is_js.push(false);
    thread.func_table.relevant_for_js.push(false);
    let resource_func = thread.func_table.length - 1;

    // Build frame for resource
    let mut resource_frame = None;
    for stack in 0..thread.stack_table.length {
        let frame = thread.stack_table.frame[stack];
        let func = thread.frame_table.func[frame];
        let resource = thread.func_table.resource[func];

        if resource == resource_to_collapse {
            thread.frame_table.func[frame] = resource_func;
            resource_frame = Some(frame);
            break;
        }
    }
    let resource_frame = resource_frame.expect("a frame for the given resource should exist");

    // We will build a new stack table, with the transformation applied.
    let mut new_stack_table: RawStackTable = RawStackTable::default();
    // Maps old stack table indices to new stack table indices, for updating the other tables.
    let mut old_stack_to_new_stack = HashMap::<IndexIntoStackTable, IndexIntoStackTable>::new();
    // let mut old_prefix_to_new_prefix = HashMap::<IndexIntoStackTable, IndexIntoStackTable>::new();
    // Lists stacks that we have already collapsed.
    let mut collapsed_stacks: HashSet<IndexIntoStackTable> = HashSet::new();

    for stack in 0..thread.stack_table.length {
        let prefix = thread.stack_table.prefix[stack];
        let frame = thread.stack_table.frame[stack];
        let func = thread.frame_table.func[frame];
        let resource = thread.func_table.resource[func];

        let new_prefix = prefix.map(|prefix| {
            *old_stack_to_new_stack
                .get(&prefix)
                .expect("all previous stack should have been mapped already")
        });

        // Check if the stack should be collapsed.
        if resource != resource_to_collapse {
            // If the stack should not be collapsed, we just copy the entry over
            // from the old stack.
            new_stack_table.length += 1;
            new_stack_table.frame.push(frame);
            new_stack_table.prefix.push(new_prefix);
            let new_stack = new_stack_table.length - 1;

            old_stack_to_new_stack.insert(stack, new_stack);
        } else {
            // If the stack should be collapsed, then we must check if the
            // prefix should be collapsed.
            if new_prefix.is_some_and(|new_prefix| collapsed_stacks.contains(&new_prefix)) {
                // If the prefix has been colapsed, we can just reuse that stack
                // entry.
                let new_prefix = new_prefix.expect("we just checked");
                old_stack_to_new_stack.insert(stack, new_prefix);
            } else {
                // If the prefix has not been colapsed, we push an entry for the
                // collapsed stack
                new_stack_table.length += 1;
                new_stack_table.frame.push(resource_frame);
                new_stack_table.prefix.push(new_prefix);
                let new_stack = new_stack_table.length - 1;

                old_stack_to_new_stack.insert(stack, new_stack);
                collapsed_stacks.insert(new_stack);
            }
        }
    }

    thread.stack_table = new_stack_table;

    for stack in &mut thread.samples.stack {
        *stack = *old_stack_to_new_stack
            .get(stack)
            .expect("all stack entries should be mapped");
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
    }

    let tree = Tree::from_profile(&profile, 0);
    println!("{}", tree);
}
