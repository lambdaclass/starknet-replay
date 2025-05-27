use std::{collections::HashMap, fmt::Display, fs::File, ops::Div, sync::LazyLock};

use itertools::Itertools;
use profiler_sdk::{model::Sample, schema::Profile};
use regex::Regex;

/// Represents the aggregated sample tree, containing the number of samples for
/// each node.
///
// NOTE: Right now, the tree is constructed all at once, by determining
// the groups that each sample belong to. I wonder if it would be better to
// construct a generic tree first, containing the full profile information,
// and transform it the way we do manually with samply. For example: "drop this
// function", "merge this node", "flatten recursion for this symbol", etc.
#[derive(Default)]
pub struct SampleTree {
    count: u64,
    children: HashMap<String, SampleTree>,
}

/// Applies the given grouper function for each sample in the profile.
///
/// The grouper function should return the heriarchy of groups that the
/// sample belongs to. For example, if the function returns `["group1",
/// "group2"]`, then the sample would be counted for `root`, `root.group1` and
/// `root.group1.group2`.
pub fn group_samples<G>(profile: &Profile, mut grouper: G) -> SampleTree
where
    G: FnMut(Sample) -> Vec<String>,
{
    let mut tree = SampleTree::default();

    /// The replay sleeps for one second before the actual execution begin
    /// We need to skip all samples up to after the sleep.
    #[derive(PartialEq, Eq)]
    enum Status {
        BeforeSleep,
        InSleep,
        AfterSleep,
    }
    let mut status = Status::BeforeSleep;

    for thread in &profile.threads {
        for sample_idx in 0..thread.samples.length {
            let sample = Sample::new(profile, thread, sample_idx);
            let symbol = sample.stack().frame().func().name();

            // If we encounter a sleep twice, something is wrong.
            if status == Status::AfterSleep && symbol == "__semwait_signal" {
                panic!()
            }
            // If we see another sample once we are sleeping, it means that we
            // finished sleeping
            if status == Status::InSleep && symbol != "__semwait_signal" {
                status = Status::AfterSleep
            }
            // Look for the first sleep
            if status == Status::BeforeSleep && symbol == "__semwait_signal" {
                status = Status::InSleep
            }
            // Only process after sleeping
            if status != Status::AfterSleep {
                continue;
            }

            let groups = grouper(sample);
            let mut current_tree = &mut tree;
            if groups.len() > 0 {
                current_tree.count += 1;
            }
            for group in groups {
                current_tree = current_tree.children.entry(group).or_default();
                current_tree.count += 1;
            }
        }
    }

    tree
}

impl Display for SampleTree {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn fmt_tree(
            tree: &SampleTree,
            f: &mut std::fmt::Formatter<'_>,
            name: &str,
            level: usize,
            total: u64,
        ) -> std::fmt::Result {
            let indent = " ".repeat(level);

            writeln!(
                f,
                "{}{:<6} - {:<5}% - {}",
                indent,
                tree.count,
                ((tree.count * 100 * 100) as f64)
                    .div(total as f64)
                    .round()
                    .div(100.0),
                name
            )?;

            let mut children = tree.children.iter().collect_vec();
            children.sort_by_key(|(_, v)| v.count);
            children.reverse();

            for (group, subtree) in children {
                fmt_tree(subtree, f, group, level + 2, total)?;
            }

            Ok(())
        }

        fmt_tree(self, f, "total", 0, self.count)
    }
}

/// Finds the crate for the given symbol.
///
/// For example, given "crate::module::function", it would return "crate".
fn find_crate_for_symbol<'p>(symbol: &'p str) -> Option<&'p str> {
    static CRATE_PATTERN: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r#"^([\w\-]+)::"#).expect("inline regex should be valid"));

    CRATE_PATTERN.captures(symbol).map(|c| {
        c.get(1)
            .expect("inline regex always has a capture group")
            .as_str()
    })
}

/// Traverses the call stack of the given sample, and returns the list of crates
/// encountered, filtering for crates that match any of `mains`.
///
/// If no matching crate is found, it returns a `["unknown"]`
fn filter_crates<'p>(sample: Sample<'p>, mains: &[&str]) -> Vec<&'p str> {
    let frame_stack = sample.stack().frame_stack();
    let mut frames = frame_stack
        .iter()
        .map(|f| f.func().name())
        .map(|s| find_crate_for_symbol(s).unwrap_or(s))
        .filter(|s| mains.iter().any(|main_crate| s.starts_with(main_crate)))
        .dedup()
        .collect_vec();

    if frames.is_empty() {
        frames.push("unknown");
    }

    frames
}

/// Traverses the call stack of the given sample, and returns the list of libs
/// encountered, filtering for libs that match any of `mains`.
///
/// If no matching lib is found, it returns a `["unknown"]`
fn filter_libs<'p>(sample: Sample<'p>, mains: &[&str]) -> Vec<&'p str> {
    let lib_stack = sample.stack().lib_stack();

    let mut libs = lib_stack
        .iter()
        .map(|f| f.name.as_str())
        .filter(|s| mains.iter().any(|main| s.starts_with(main)))
        .dedup()
        .collect_vec();

    if libs.is_empty() {
        libs.push(&lib_stack[0].name);
    }

    libs
}

/// Traverses the frame stack of the given sample, and returns the list of
/// libs or crates encountered, filtering for libs or crates that match any of
/// `mains`. If both crate and lib match, only keeps the crate name
///
/// If no matching crate or lib is found, it returns a `["unknown"]`
fn filter_crates_and_libs<'p>(sample: Sample<'p>, crates: &[&str], libs: &[&str]) -> Vec<&'p str> {
    let frame_stack = sample.stack().frame_stack();

    let mut sources = frame_stack
        .iter()
        .filter_map(|frame| {
            let symbol = frame.func().name();

            if let Some(ccrate) = find_crate_for_symbol(symbol) {
                if crates.iter().any(|c| ccrate.starts_with(c)) {
                    return Some(ccrate);
                }
            }

            let lib = &frame.native_symbol().lib().name;
            if libs.iter().any(|l| lib.starts_with(l)) {
                Some(lib)
            } else {
                None
            }
        })
        .dedup()
        .collect_vec();

    if sources.is_empty() {
        sources.push("unknown");
    }

    sources
}

fn section<G>(title: &str, profile: &Profile, grouper: G)
where
    G: FnMut(Sample) -> Vec<String>,
{
    println!("{}", "=".repeat(title.len()));
    println!("{}", title);
    println!("{}", "=".repeat(title.len()));
    println!();
    println!("{}", group_samples(&profile, grouper));
}

fn main() {
    let path = &std::env::args().collect_vec()[1];
    let file = File::open(path).expect("failed to open profile");
    let profile: Profile = serde_json::from_reader(file).expect("failed to deserialize profile");

    section("Samples by Library", &profile, |sample| {
        vec![filter_libs(sample, &["replay", "0x"])[0].to_string()]
    });

    section("Samples by Crate", &profile, |sample| {
        let crates = filter_crates(
            sample,
            &[
                "blockifier",
                "cairo_native",
                "replay",
                "lambdaworks",
                "starknet",
            ],
        );
        if crates[0] == "replay" {
            vec![]
        } else {
            vec![crates[0].to_string()]
        }
    });

    section("Samples by Source", &profile, |sample| {
        let sources =
            filter_crates_and_libs(sample, &["replay", "blockifier", "cairo_native"], &["0x"]);

        if sources[0].starts_with("0x") {
            return vec!["MLIR".to_string()];
        }

        if sources[0] == "cairo_native" && sources[1].starts_with("0x") {
            let mut groups = vec!["Runtime".to_string()];

            let frame_stack = sample.stack().frame_stack();

            let last_mlir_frame = frame_stack
                .iter()
                .position(|frame| frame.native_symbol().lib().name.starts_with("0x"))
                .expect("should find an MLIR frame eventually");

            let first_runtime_symbol = frame_stack[last_mlir_frame - 1].func().name();

            static FUNCTION_PATTERN: LazyLock<Regex> =
                LazyLock::new(|| Regex::new(r#".*::([\w\-]+)"#).unwrap());

            let first_runtime_func = FUNCTION_PATTERN
                .captures(first_runtime_symbol)
                .map(|c| c.get(1).unwrap().as_str())
                .unwrap_or(first_runtime_symbol)
                .to_string();

            groups.push(first_runtime_func);

            return groups;
        }

        if sources[0] == "replay" {
            vec![]
        } else {
            vec![sources[0].to_string()]
        }
    });
    section("Samples by Crate Call", &profile, |sample| {
        let mut sources =
            filter_crates_and_libs(sample, &["replay", "blockifier", "cairo_native"], &["0x"]);

        for source in &mut sources {
            if source.starts_with("0x") {
                *source = "MLIR"
            }
        }

        if sources[0] == "replay" {
            vec![]
        } else if sources[0] == "unknown" {
            sources.iter().map(|s| s.to_string()).collect_vec()
        } else {
            sources[..2]
                .iter()
                .map(|s| s.to_string())
                .rev()
                .collect_vec()
        }
    });
}
