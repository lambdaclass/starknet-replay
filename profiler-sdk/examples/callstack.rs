use std::fs::File;

use profiler_sdk::{schema::Profile, tree::Tree};

pub fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("expected profile path as first argument");
    let file = File::open(path).expect("failed to open profile");
    let profile: Profile = serde_json::from_reader(file).expect("failed to deserialize profile");
    let tree = Tree::from_profile(&profile, 0);

    println!("{}", tree);
}
