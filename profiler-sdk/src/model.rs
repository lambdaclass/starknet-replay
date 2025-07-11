//! This module defines entities for traversing the data structure, without
//! dealing with it directly. They only encode the data layout, and do not store
//! attributes
//!
//! As an example, lets suppose we want to find the function name for
//! a given `thread_idx` and `sample_idx`.
//!
//! ```no_run
//! # use profiler_sdk::schema::Profile;
//! # use profiler_sdk::model::Sample;
//! # let profile: Profile = todo!();
//! # let thread_idx = 0;
//! # let sample_idx = 23;
//! let thread = &profile.threads[thread_idx];
//! let sample = Sample::new(&profile, thread, sample_idx);
//! let name: &str = sample.stack().frame().func().name();
//! ```

use itertools::Itertools;

use crate::schema::{
    IndexIntoFrameTable, IndexIntoFuncTable, IndexIntoLibs, IndexIntoNativeSymbolTable,
    IndexIntoResourceTable, IndexIntoSampleTable, IndexIntoStackTable, Lib, Profile, RawThread,
};

#[derive(Copy, Clone)]
pub struct Sample<'p> {
    profile: &'p Profile,
    thread: &'p RawThread,
    idx: IndexIntoSampleTable,
}

#[derive(Copy, Clone)]
pub struct Stack<'p> {
    profile: &'p Profile,
    thread: &'p RawThread,
    idx: IndexIntoStackTable,
}

#[derive(Copy, Clone)]
pub struct Frame<'p> {
    pub profile: &'p Profile,
    pub thread: &'p RawThread,
    idx: IndexIntoFrameTable,
}

#[derive(Copy, Clone)]
pub struct Func<'p> {
    profile: &'p Profile,
    thread: &'p RawThread,
    pub idx: IndexIntoFuncTable,
}

#[derive(Copy, Clone)]
pub struct NativeSymbol<'p> {
    profile: &'p Profile,
    thread: &'p RawThread,
    idx: IndexIntoNativeSymbolTable,
}

impl<'p> Sample<'p> {
    pub fn new(profile: &'p Profile, thread: &'p RawThread, idx: IndexIntoSampleTable) -> Self {
        Self {
            profile,
            thread,
            idx,
        }
    }

    pub fn stack(&self) -> Stack<'p> {
        Stack::new(
            self.profile,
            self.thread,
            self.thread.samples.stack[self.idx],
        )
    }

    pub fn weight(&self) -> u64 {
        self.thread.samples.weight[self.idx]
    }
}

impl<'p> Stack<'p> {
    pub fn new(profile: &'p Profile, thread: &'p RawThread, idx: IndexIntoStackTable) -> Self {
        Self {
            profile,
            thread,
            idx,
        }
    }

    pub fn frame(&self) -> Frame<'p> {
        Frame::new(
            self.profile,
            self.thread,
            self.thread.stack_table.frame[self.idx],
        )
    }

    pub fn prefix(&self) -> Option<Stack<'p>> {
        self.thread.stack_table.prefix[self.idx]
            .map(|prefix_idx| Stack::new(self.profile, self.thread, prefix_idx))
    }

    // TODO: Make this lazy. We sometime only need the first frames, and not the entire stack.
    pub fn frame_stack(&self) -> Vec<Frame<'p>> {
        let mut frames = Vec::new();

        let mut current_stack = *self;
        loop {
            let frame = current_stack.frame();
            frames.push(frame);

            if let Some(prefix_stack) = current_stack.prefix() {
                current_stack = prefix_stack
            } else {
                break;
            }
        }

        frames
    }

    pub fn symbol_stack(&self) -> Vec<NativeSymbol> {
        self.frame_stack()
            .into_iter()
            .filter_map(|frame| frame.native_symbol())
            .collect_vec()
    }

    pub fn lib_stack(&self) -> Vec<&'p Lib> {
        self.symbol_stack()
            .iter()
            .map(|x| x.lib_idx())
            .dedup()
            .map(|x| &self.profile.libs[x])
            .collect_vec()
    }
}

impl<'p> Frame<'p> {
    pub fn new(profile: &'p Profile, thread: &'p RawThread, idx: IndexIntoFrameTable) -> Self {
        Self {
            profile,
            thread,
            idx,
        }
    }

    pub fn func(&self) -> Func<'p> {
        Func::new(
            self.profile,
            self.thread,
            self.thread.frame_table.func[self.idx],
        )
    }
    pub fn native_symbol(&self) -> Option<NativeSymbol<'p>> {
        Some(NativeSymbol::new(
            self.profile,
            self.thread,
            self.thread.frame_table.native_symbol[self.idx]?,
        ))
    }
}

impl<'p> Func<'p> {
    pub fn new(profile: &'p Profile, thread: &'p RawThread, idx: IndexIntoFuncTable) -> Self {
        Self {
            profile,
            thread,
            idx,
        }
    }

    pub fn name(&self) -> &'p str {
        let name_idx = self.thread.func_table.name[self.idx];
        &self.profile.shared.string_array[name_idx]
    }

    pub fn resource_idx(&self) -> Option<IndexIntoResourceTable> {
        self.thread.func_table.resource[self.idx].into()
    }
}

impl<'p> NativeSymbol<'p> {
    pub fn new(
        profile: &'p Profile,
        thread: &'p RawThread,
        idx: IndexIntoNativeSymbolTable,
    ) -> Self {
        Self {
            profile,
            thread,
            idx,
        }
    }

    pub fn lib_idx(&self) -> IndexIntoLibs {
        self.thread.native_symbols.lib_index[self.idx]
    }

    pub fn lib(&self) -> &'p Lib {
        &self.profile.libs[self.lib_idx()]
    }
}
