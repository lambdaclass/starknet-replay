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
    pub subtree: Tree<'p>,
}

impl<'p> Tree<'p> {
    pub fn from_profile(profile: &'p Profile, thread_idx: usize) -> Self {
        let mut tree = Tree::default();

        let thread = &profile.threads[thread_idx];
        for sample_idx in 0..thread.samples.length {
            let sample = Sample::new(profile, thread, sample_idx);

            let frames = sample.stack().frame_stack().into_iter().rev();

            let mut tree = &mut tree;
            for frame in frames {
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
                        });
                        tree.children.len() - 1
                    }
                };

                let subtree = &mut tree.children[subtree_index];
                subtree.count += sample.weight();

                tree = &mut subtree.subtree;
            }
        }

        tree
    }
}

impl<'p> Display for Tree<'p> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        fn count_inner(node: &Node) -> u64 {
            node.subtree
                .children
                .iter()
                .map(|n| count_inner(n))
                .sum::<u64>()
                + node.count
        }

        fn inner<'p>(
            f: &mut std::fmt::Formatter<'_>,
            node: &Node<'p>,
            total: u64,
            prefix: &str,
            marker: &str,
        ) -> std::fmt::Result {
            let subtotal = node.subtree.children.iter().map(count_inner).sum::<u64>() + node.count;

            let percentage = subtotal as f64 / total as f64 * 100.0;

            writeln!(
                f,
                "│ {:<5.1} │ {:<5} │ {:<5} │ {}{}{}",
                percentage, subtotal, node.count, prefix, marker, node.name
            )?;

            let new_prefix = format!("{}{}", prefix, {
                if marker.is_empty() {
                    format!("")
                } else if marker == "├─ " {
                    format!("│  ")
                } else {
                    format!("   ")
                }
            });

            let mut children = node.subtree.children.iter().peekable();
            while let Some(child) = children.next() {
                let new_marker = if children.peek().is_none() {
                    format!("└─ ")
                } else {
                    format!("├─ ")
                };

                inner(f, child, total, &new_prefix, &new_marker)?;
            }

            Ok(())
        }

        writeln!(f, "│ RATIO │ TOTAL │ SELF  │ TREE",)?;
        writeln!(f, "│       │       │       │     ",)?;

        let total = self.children.iter().map(count_inner).sum::<u64>();

        for children in &self.children {
            inner(f, children, total, "", "")?;
        }

        Ok(())
    }
}
