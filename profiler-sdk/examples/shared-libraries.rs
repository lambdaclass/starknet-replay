use std::fs::File;

use profiler_sdk::{schema::Profile, transforms::collapse_resource, tree::Tree};

pub fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("expected profile path as first argument");
    let file = File::open(path).expect("failed to open profile");
    let mut profile: Profile =
        serde_json::from_reader(file).expect("failed to deserialize profile");

    // Collapse all resources except contract shared libraries.
    for resource_idx in 0..profile.threads[0].resource_table.length {
        collapse_resource(&mut profile, 0, resource_idx);
    }

    let tree = Tree::from_profile(&profile, 0);

    println!("{}", tree);
}
