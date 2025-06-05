use std::{fs::File, path::PathBuf};

use blockifier::execution::native::executor::{ProfilerResults, LIBFUNC_PROFILES_MAP};
use serde::Serialize;
use starknet_types_core::felt::Felt;

#[derive(Serialize)]
struct LibfuncProfile {
    block_number: u64,
    tx: Felt,
    data: Vec<ProfilerResults>,
}

pub fn create_libfunc_profile(block_number: u64, tx_hash_str: &str) {
    let mut profiles = LIBFUNC_PROFILES_MAP.lock().unwrap();
    let root = PathBuf::from(format!("libfunc_profiles/block{block_number}"));

    std::fs::create_dir_all(&root).unwrap();

    let mut path = root.join(tx_hash_str);
    path.set_extension("json");

    let profile_file = File::create(path).unwrap();

    let profiles_data = profiles
        .iter()
        .map(|(k, v)| LibfuncProfile {
            block_number,
            tx: *k,
            data: v.clone(),
        })
        .collect::<Vec<_>>();

    serde_json::to_writer_pretty(profile_file, &profiles_data).unwrap();

    profiles.clear();
}
