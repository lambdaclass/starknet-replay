//! Contains definitions for deserializing samples from the firefox profiler in
//! preprocessed profile format. *Not* in Gecko profile format.
//!
//! This file was translated from firefox profiler source code:
//! - https://github.com/firefox-devtools/profiler/blob/main/src/types/profile.js
//!
//! # Data Model
//!
//! The profile is organized in tables for each entity, and indeces to link
//! entities together. A sample is identified by a thread index, and a sample
//! index.
//!
//! As an example, lets suppose we want to find the function name for
//! a given `thread_idx` and `sample_idx`.
//!
//! ```no_run
//! # use profiler_sdk::schema::Profile;
//! # let profile: Profile = todo!();
//! # let thread_idx = 0;
//! # let sample_idx = 23;
//! let thread = &profile.threads[thread_idx];
//! let stack_idx = thread.samples.stack[sample_idx];
//! let frame_idx = thread.stack_table.frame[stack_idx];
//! let func_idx = thread.frame_table.func[frame_idx];
//! let name_idx = thread.func_table.name[func_idx];
//! let name: String = thread.string_array[name_idx];
//! ```

use serde::Deserialize;
use serde_json::Value;

pub type Uint = u64;
pub type Milliseconds = f64;
pub type Address = u64;
pub type Bytes = u64;

pub type IndexIntoSampleTable = usize;
pub type IndexIntoStackTable = usize;
pub type IndexIntoFrameTable = usize;
pub type IndexIntoStringTable = usize;
pub type IndexIntoFuncTable = usize;
pub type IndexIntoResourceTable = usize;
pub type IndexIntoLibs = usize;
pub type IndexIntoNativeSymbolTable = usize;

pub type ResourceTypeEnum = Uint;

/// All of the data for a processed profile.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Profile {
    pub meta: ProfileMeta,
    pub libs: Vec<Lib>,
    pub threads: Vec<RawThread>,
    pub pages: Vec<()>,
    pub profiler_overhead: Vec<()>,
    pub counters: Vec<()>,
}

/// Meta information associated for the entire profile.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ProfileMeta {
    pub debug: bool,
    /// The interval at which the threads are sampled.
    pub interval: Milliseconds,
    /// The OS and CPU. e.g. "Intel Mac OS X"
    pub oscpu: Option<String>,
    /// This is the processed profile format version.
    pub preprocessed_profile_version: Uint,
    /// The name of the product, most likely "Firefox".
    pub product: String,
    /// Units of samples table values.
    pub sample_units: SampleUnits,
    /// When the main process started. Timestamp expressed in milliseconds since
    /// midnight January 1, 1970 GMT.
    pub start_time: Milliseconds,
    /// A bool flag indicating whether we symbolicated this profile. If this is
    /// false we'll start a symbolication process when the profile is loaded.
    /// A missing property means that it's an older profile, it stands for an
    /// "unknown" state.  For now we don't do much with it but we may want to
    /// propose a manual symbolication in the future.
    pub symbolicated: bool,
    /// This is the Gecko profile format version (the unprocessed version
    /// received directly from the browser.)
    pub version: Uint,
    pub categories: Vec<Value>,
    pub extensions: Value,
    pub process_type: Value,
    pub paused_ranges: Vec<()>,
    pub marker_schema: Vec<()>,
    #[serde(default)]
    pub uses_only_one_stack_type: bool,
    #[serde(default)]
    pub source_code_is_not_on_searchfox: bool,
}

/// The thread type. Threads are stored in an array in profile.threads.
///
/// If a profile contains threads from different OS-level processes, all threads
/// are flattened into the single threads array, and per-process information
/// is duplicated on each thread. In the UI, we recover the process separation
/// based on thread.pid.
///
/// There is also a derived `Thread` type, see profile-derived.js.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct RawThread {
    pub process_type: String,
    pub process_name: String,
    pub name: String,
    pub is_main_thread: bool,
    pub pid: Pid,
    pub tid: Tid,
    pub process_startup_time: Milliseconds,
    pub process_shutdown_time: Milliseconds,
    pub register_time: Milliseconds,
    pub unregister_time: Milliseconds,
    pub samples: RawSamplesTable,
    pub stack_table: RawStackTable,
    pub frame_table: FrameTable,
    pub func_table: FuncTable,
    pub resource_table: ResourceTable,
    pub native_symbols: NativeSymbolTable,
    /// Strings for profiles are collected into a single table, and are referred
    /// to by their index by other tables.
    pub string_array: Vec<String>,
    pub markers: Value,
    pub paused_ranges: Vec<()>,
    #[serde(default)]
    pub show_markers_in_timeline: bool,
}

/// The Gecko Profiler records samples of what function was currently being
/// executed, and the callstack that is associated with it. This is done at a
/// fixed but configurable rate, e.g. every 1 millisecond. This table represents
/// the minimal amount of information that is needed to represent that sampled
/// function. Most of the entries are indices into other tables.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct RawSamplesTable {
    pub length: usize,
    pub stack: Vec<IndexIntoStackTable>,
    pub time: Vec<Milliseconds>,
    /// An optional weight array. If not present, then the weight is assumed to
    /// be 1. See the WeightType type for more information.
    pub weight: Vec<Uint>,
    /// CPU usage value of the current thread. Its values are null only if the
    /// back-end fails to get the CPU usage from operating system. It's landed
    /// in Firefox 86, and it is optional because older profile versions may not
    /// have it or that feature could be disabled. No upgrader was written for
    /// this change because it's a completely new data source. The first value
    /// is ignored - it's not meaningful because there is no previous sample.
    #[serde(rename = "threadCPUDelta")]
    pub thread_cpu_delta: Vec<Uint>,
    pub weight_type: String,
}

/// The stack table stores the tree of stack nodes of a thread. The shape of
/// the tree is encoded in the prefix column: Root stack nodes have null as
/// their prefix, and every non-root stack has the stack index of its "caller"
/// / "parent" as its prefix. Every stack node also has a frame and a category.
/// A "call stack" is a list of frames. Every stack index in the stack table
/// represents such a call stack; the "list of frames" is obtained by walking
/// the path in the tree from the root to the given stack node.
///
/// Stacks are used in the thread's samples; each sample refers to a stack
/// index. Stacks can be shared between samples.
///
/// With this representation, every sample only needs to store a single integer
/// to identify the sample's stack. We take advantage of the fact that many call
/// stacks in the profile have a shared prefix; storing these stacks as a tree
/// saves a lot of space compared to storing them as actual lists of frames.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct RawStackTable {
    pub frame: Vec<IndexIntoFrameTable>,
    pub prefix: Vec<Option<IndexIntoStackTable>>,
    pub length: usize,
}

/// Frames contain the context information about the function execution at the
/// moment in time. The caller/callee relationship between frames is defined by
/// the StackTable.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct FrameTable {
    pub length: usize,
    /// If this is a frame for native code, the address is the address of
    /// the frame's assembly intruction,  relative to the native library that
    /// contains it.
    ///
    /// For frames obtained from stack walking, the address points into the
    /// call intruction. It is not a return address, it is a "nudged" return
    /// address (i.e. return address minus one byte). This is different from
    /// the Gecko profile format. The conversion is performed at the end of
    /// profile processing. See the big comment above nudgeReturnAddresses for
    /// more details.
    ///
    /// The library which this address is relative to is given by the frame's
    /// nativeSymbol: frame -> nativeSymbol -> lib.
    pub address: Vec<Address>,
    /// The inline depth for this frame. If there is an inline stack at an
    /// address, we create multiple frames with the same address, one for each
    /// depth. The outermost frame always has depth 0.
    ///
    /// # Example
    ///
    /// If the raw stack is 0x10 -> 0x20 -> 0x30, and symbolication
    /// adds two inline frames for 0x10, no inline frame for 0x20, and
    /// one inline frame for 0x30, then the symbolicated stack will be the
    /// following:
    ///
    /// func:        outer1 -> inline1a -> inline1b -> outer2 -> outer3 -> inline3a
    /// address:     0x10   -> 0x10     -> 0x10     -> 0x20   -> 0x30   -> 0x30
    /// inlineDepth:    0   ->    1     ->    2     ->    0   ->    0   ->    1
    ///
    /// # Background
    ///
    /// When a compiler performs an inlining optimization, it removes a call
    /// to a function and instead generates the code for the called function
    /// directly into the outer function. But it remembers which intructions
    /// were the result of this inlining, so that information about the
    /// inlined function can be recovered from the debug information during
    /// symbolication, based on the intruction address. The compiler can choose
    /// to do inlining multiple levels deep: An intruction can be the result of
    /// a whole "inline stack" of functions. Before symbolication, all frames
    /// have depth 0. During symbolication, we resolve addresses to inline
    /// stacks, and create extra frames with non-zero depths as needed.
    ///
    /// The frames of an inline stack at an address all have the same address
    /// and the same nativeSymbol, but each has a different func and line.
    pub inline_depth: Vec<Uint>,
    /// The frame's function.
    pub func: Vec<IndexIntoFuncTable>,
    /// The symbol index (referring into this thread's nativeSymbols table)
    /// corresponding to symbol that covers the frame address of this frame.
    /// Only non-null for native frames (e.g. C / C++ / Rust code). Null before
    /// symbolication.
    pub native_symbol: Vec<IndexIntoNativeSymbolTable>,
    pub line: Vec<Option<Uint>>,
    pub column: Vec<Option<Uint>>,
    pub category: Value,
    pub subcategory: Value,
    #[serde(rename = "innerWindowID")]
    pub inner_window_id: Value,
}

/// The funcTable stores the functions that were called in the profile.
/// These can be native functions (e.g. C / C++ / rust), JavaScript functions, or
/// "label" functions. Multiple frames can have the same function: The frame
/// represents which part of a function was being executed at a given moment, and
/// the function groups all frames that occurred inside that function.
/// Concretely, for native code, each encountered intruction address is a separate
/// frame, and the function groups all intruction addresses which were symbolicated
/// with the same function name.
/// For JS code, each encountered line/column in a JS file is a separate frame, and
/// the function represents an entire JS function which can span multiple lines.
///
/// Funcs that are orphaned, i.e. funcs that no frame refers to, do not have
/// meaningful values in their fields. Symbolication will cause many funcs that
/// were created upfront to become orphaned, as the frames that originally referred
/// to them get reassigned to the canonical func for their actual function.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct FuncTable {
    pub length: usize,
    /// The function name.
    pub name: Vec<IndexIntoStringTable>,
    /// The resource describes "Which bag of code did this function come from?".
    /// For JS functions, the resource is of type addon, webhost, otherhost, or
    /// url. For native functions, the resource is of type library. For labels
    /// and for other unidentified functions, we set the resource to -1.
    pub resource: Vec<IndexIntoResourceTable>,
    /// These are non-null for JS functions only. The line and column describe
    /// the location of the *start* of the JS function. As for the information
    /// about which which lines / columns inside the function were actually hit
    /// during execution, that information is stored in the frameTable, not in
    /// the funcTable.
    pub file_name: Vec<Option<IndexIntoStringTable>>,
    pub line_number: Vec<Option<Uint>>,
    pub column_number: Vec<Option<Uint>>,
    #[serde(rename = "isJS")]
    pub is_js: Vec<bool>,
    #[serde(rename = "relevantForJS")]
    pub relevant_for_js: Vec<bool>,
}

/// The nativeSymbols table stores the addresses and symbol names for all
/// symbols that were encountered by frame addresses in this thread. This
/// table can contain symbols from multiple libraries, and the symbols are in
/// arbitrary order.
///
/// Note: Despite the similarity in name, this table is not
/// what's usually considered a "symbol table" - normally, a "symbol table" is
/// something that contains *all* symbols of a given library. But this table
/// only contains a subset of those symbols, and mixes symbols from multiple
/// libraries.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct NativeSymbolTable {
    pub length: usize,
    /// The library that this native symbol is in.
    pub lib_index: Vec<IndexIntoLibs>,
    /// The library-relative offset of this symbol.
    pub address: Vec<Address>,
    /// The symbol name, demangled.
    pub name: Vec<IndexIntoStringTable>,
    /// The size of the function's machine code (if known), in bytes.
    pub function_size: Vec<Option<Uint>>,
}

/// The ResourceTable holds additional information about functions. It tends to
/// contain sparse arrays. Multiple functions can point to the same resource.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct ResourceTable {
    pub length: usize,
    pub lib: Vec<IndexIntoLibs>,
    pub name: Vec<IndexIntoStringTable>,
    pub host: Vec<Option<IndexIntoStringTable>>,
    pub r#type: Vec<ResourceTypeEnum>,
}

/// Information about the shared libraries that were loaded into the processes
/// in the profile. This information is needed during symbolication. Most
/// importantly, the symbolication API requires a debugName + breakpadId for
/// each set of unsymbolicated addresses, to know where to obtain symbols for
/// those addresses.
#[derive(Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct Lib {
    /// e.g. "x86_64,
    pub arch: String,
    /// e.g. "firefox,
    pub name: String,
    /// e.g. "/Applications/FirefoxNightly.app/Contents/MacOS/firefox,
    pub path: String,
    /// e.g. "firefox", or "firefox.pdb" on Window,
    pub debug_name: String,
    /// e.g. "/Applications/FirefoxNightly.app/Contents/MacOS/firefox,
    pub debug_path: String,
    /// e.g. "E54D3AF274383256B9F6144F83F3F7510,
    pub breakpad_id: String,
    pub code_id: Option<String>,
}

/// Object that holds the units of samples table values. Some of the values can be
/// different depending on the platform, e.g. threadCPUDelta.
/// See https://searchfox.org/mozilla-central/rev/851bbbd9d9a38c2785a24c13b6412751be8d3253/tools/profiler/core/platform.cpp#2601-2606
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub struct SampleUnits {
    pub time: TimelineUnit,
    #[serde(rename = "threadCPUDelta")]
    pub thread_cpu_delta: ThreadCPUDeltaUnit,
    pub event_delay: Value,
}

/// Unit of the values in the timeline. Used to differentiate size-profiles.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub enum TimelineUnit {
    Ms,
    Bytes,
}

/// Units of ThreadCPUDelta values for different platforms.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(deny_unknown_fields)]
pub enum ThreadCPUDeltaUnit {
    Ns,
    #[serde(rename = "µs")]
    Us,
    Variable,
}

/// The Tid is most often a Number. However in some cases such as merged
/// profiles we could generate a String.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Tid {
    #[serde(untagged)]
    Number(Uint),
    #[serde(untagged)]
    String(String),
}

/// Pids are Strings, often Stringified Numbers. Strings allow creating unique
/// values when multiple processes with the same pid exist in the same profile,
/// such as during profile merging or diffing.
pub type Pid = String;
