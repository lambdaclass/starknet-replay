use std::{collections::HashMap, fs::File, path::PathBuf};

use blockifier::execution::native::executor::LIBFUNC_COUNTERS_MAP;
use cairo_lang_sierra::{
    extensions::core::{CoreLibfunc, CoreType},
    program_registry::ProgramRegistry,
};
use cairo_native::debug::libfunc_to_name;
use serde::Serialize;
use starknet_types_core::felt::Felt;

type CounterByLibfunc = HashMap<String, u32>;

#[derive(Serialize)]
struct TxLibfuncCounter {
    block_number: u64,
    tx: String,
    entrypoints: Vec<CountersByEntrypoint>,
}

#[derive(Serialize)]
struct CountersByEntrypoint {
    class_hash: Felt,
    selector: Felt,
    entrypoint_counters: CounterByLibfunc,
}

pub fn create_libfunc_counter(tx_hash_str: String) {
    let mut counter = LIBFUNC_COUNTERS_MAP.lock().unwrap();
    let tx_counter = counter
        .remove(&tx_hash_str)
        .unwrap_or_else(|| panic!("tx with hash: {} should not be None", tx_hash_str));
    let entrypoints = tx_counter
        .entrypoint_counters
        .iter()
        .map(|entrypoint| {
            let registry: ProgramRegistry<CoreType, CoreLibfunc> =
                ProgramRegistry::new(&entrypoint.program).unwrap();
            let entrypoint_counters = entrypoint
                .counters
                .iter()
                .enumerate()
                .map(|(i, count)| {
                    let libfunc = &entrypoint.program.libfunc_declarations[i];
                    let libfunc = registry.get_libfunc(&libfunc.id).unwrap();

                    (libfunc_to_name(&libfunc).to_string(), *count)
                })
                .collect::<HashMap<String, u32>>();

            CountersByEntrypoint {
                class_hash: entrypoint.class_hash,
                selector: entrypoint.selector,
                entrypoint_counters,
            }
        })
        .collect::<Vec<_>>();

    let tx_libfunc_counter = TxLibfuncCounter {
        block_number: tx_counter.block_number,
        tx: tx_counter.tx_hash,
        entrypoints,
    };

    let root = PathBuf::from(format!("libfunc_counts/block{}", tx_counter.block_number));

    std::fs::create_dir_all(&root).unwrap();

    let mut path = root.join(tx_hash_str);
    path.set_extension("json");

    let counter_file = File::create(path).unwrap();

    serde_json::to_writer_pretty(counter_file, &tx_libfunc_counter).unwrap();
}
