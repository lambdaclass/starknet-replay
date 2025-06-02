use std::fmt::Display;

use crate::{model::Sample, schema::Profile};

#[derive(Debug, Default)]
pub struct Tree<'p> {
    pub children: Vec<Node<'p>>,
}

#[derive(Debug)]
pub struct Node<'p> {
    pub name: &'p str,
    pub count: u64,
    pub subtotal: u64,
    pub subtree: Tree<'p>,
}

impl<'p> Tree<'p> {
    pub fn from_profile(profile: &'p Profile, thread_idx: usize) -> Self {
        let mut tree = Tree::default();

        let thread = &profile.threads[thread_idx];
        for sample_idx in 0..thread.samples.length {
            let sample = Sample::new(profile, thread, sample_idx);

            let mut frames = sample.stack().frame_stack().into_iter().rev().peekable();

            let mut tree = &mut tree;
            while let Some(frame) = frames.next() {
                let symbol = frame.func().name();

                let subtree_index = tree
                    .children
                    .iter()
                    .position(|subtree| subtree.name == symbol);
                let subtree_index = match subtree_index {
                    Some(subtree_index) => subtree_index,
                    None => {
                        tree.children.push(Node {
                            name: symbol,
                            count: 0,
                            subtree: Default::default(),
                            subtotal: 0,
                        });
                        tree.children.len() - 1
                    }
                };

                let subtree = &mut tree.children[subtree_index];

                subtree.subtotal += sample.weight();
                if frames.peek().is_none() {
                    subtree.count += sample.weight();
                }

                tree = &mut subtree.subtree;

                tree.children.sort_by_key(|n| n.subtotal);
                tree.children.reverse();
            }
        }

        tree
    }
}

/// Tree display mimicks how the firefox profiler does it:
///
///
/// ```
/// │ RATIO │ TOTAL │ SELF  │ TREE
/// │       │       │       │
/// │ 100.0 │ 46    │ 5     │ 0x6b97
/// │ 89.1  │ 41    │ 5     │ └─ 0x62ebb
/// │ 78.3  │ 36    │ 5     │    └─ 0x19c9b
/// │ 67.4  │ 31    │ 5     │       └─ 0x462cb
/// │ 56.5  │ 26    │ 5     │          └─ 0x435e3
/// │ 19.6  │ 9     │ 1     │             ├─ 0x3e3e7
/// │ 17.4  │ 8     │ 1     │             │  └─ 0xf86c7
/// │ 15.2  │ 7     │ 1     │             │     └─ 0x1c31d7
/// │ 13.0  │ 6     │ 1     │             │        └─ 0x1c91f3
/// │ 10.9  │ 5     │ 1     │             │           └─ 0xf9417
/// │ 8.7   │ 4     │ 1     │             │              └─ 0x12aaa3
/// │ 6.5   │ 3     │ 1     │             │                 └─ 0x12e6f3
/// │ 4.3   │ 2     │ 1     │             │                    └─ 0x169d8b
/// │ 2.2   │ 1     │ 1     │             │                       └─ 0x16b5c4
/// │ 26.1  │ 12    │ 4     │             └─ 0x41103
/// │ 17.4  │ 8     │ 4     │                └─ 0x90af
/// │ 8.7   │ 4     │ 4     │                   └─ 0x29b8
/// ```
impl Display for Tree<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn inner(
            f: &mut std::fmt::Formatter<'_>,
            node: &Node<'_>,
            total: u64,
            prefix: &str,
            marker: &str,
        ) -> std::fmt::Result {
            let percentage = node.subtotal as f64 / total as f64 * 100.0;

            writeln!(
                f,
                "│ {:<5.1} │ {:<7} │ {:<7} │ {}{}{}",
                percentage, node.subtotal, node.count, prefix, marker, node.name
            )?;

            let new_prefix = format!("{}{}", prefix, {
                if marker.is_empty() {
                    "".to_string()
                } else if marker == "├─ " {
                    "│  ".to_string()
                } else {
                    "   ".to_string()
                }
            });

            let mut children = node.subtree.children.iter().peekable();

            while let Some(child) = children.next() {
                let new_marker = if children.peek().is_none() {
                    "└─ ".to_string()
                } else {
                    "├─ ".to_string()
                };

                inner(f, child, total, &new_prefix, &new_marker)?;
            }

            Ok(())
        }

        writeln!(f, "│ RATIO │  TOTAL  │  SELF   │ TREE",)?;
        writeln!(f, "│       │         │         │     ",)?;

        let total = self.children.iter().map(|n| n.subtotal).sum::<u64>();

        for children in &self.children {
            inner(f, children, total, "", "")?;
        }

        Ok(())
    }
}
