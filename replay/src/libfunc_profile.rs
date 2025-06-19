use std::{fs::File, path::PathBuf};

use blockifier::execution::native::executor::{
    EntrypointProfile, TransactionProfile, LIBFUNC_PROFILES_MAP,
};
use cairo_lang_sierra::{
    extensions::core::{CoreLibfunc, CoreType},
    ids::ConcreteLibfuncId,
    program::Program,
    program_registry::ProgramRegistry,
};
use cairo_native::{debug::libfunc_to_name, metadata::profiler::LibfuncProfileData};
use serde::Serialize;
use starknet_types_core::felt::Felt;

#[derive(Serialize)]
struct TxLibfuncProfileSummary {
    block_number: u64,
    tx: String,
    entrypoints: Vec<ProcessedEntrypointProfile>,
}

#[derive(Serialize)]
struct ProcessedEntrypointProfile {
    class_hash: Felt,
    selector: Felt,
    profile_summary: Vec<LibfuncProfileSummary>,
}
#[derive(Clone, Debug, Serialize)]
pub struct LibfuncProfileSummary {
    pub libfunc_name: String,
    pub samples: u64,
    pub total_time: Option<u64>,
    pub average_time: Option<f64>,
    pub std_deviation: Option<f64>,
    pub quartiles: Option<[u64; 5]>,
}

pub fn create_libfunc_profile(tx_hash_str: String) {
    let mut profiles = LIBFUNC_PROFILES_MAP.lock().unwrap();
    let tx_profile = profiles
        .remove(&tx_hash_str)
        .unwrap_or_else(|| panic!("tx with hash: {} should not be None", tx_hash_str));
    let root = PathBuf::from(format!("libfunc_profiles/block{}", tx_profile.block_number));

    std::fs::create_dir_all(&root).unwrap();

    let mut path = root.join(tx_hash_str);
    path.set_extension("json");

    let profile_file = File::create(path).unwrap();

    let processed_profile = process_tx_profile(tx_profile).unwrap();

    serde_json::to_writer_pretty(profile_file, &processed_profile).unwrap();
}

fn process_tx_profile(
    TransactionProfile {
        block_number,
        tx_hash,
        mut entrypoint_profiles,
    }: TransactionProfile,
) -> anyhow::Result<TxLibfuncProfileSummary> {
    let entrypoints = entrypoint_profiles
        .iter_mut()
        .map(
            |EntrypointProfile {
                 class_hash,
                 selector,
                 profile,
                 program,
             }| {
                let profile_summary = profile
                    .iter_mut()
                    .map(|data| process_entrypoint_profile(data, program))
                    .collect::<anyhow::Result<Vec<_>>>()?;

                Ok(ProcessedEntrypointProfile {
                    class_hash: *class_hash,
                    selector: *selector,
                    profile_summary,
                })
            },
        )
        .collect::<anyhow::Result<Vec<_>>>()?;

    Ok(TxLibfuncProfileSummary {
        block_number,
        tx: tx_hash,
        entrypoints,
    })
}

fn process_entrypoint_profile(
    (libfunc_id, data): (&ConcreteLibfuncId, &mut LibfuncProfileData),
    program: &Program,
) -> anyhow::Result<LibfuncProfileSummary> {
    let registry: ProgramRegistry<CoreType, CoreLibfunc> = ProgramRegistry::new(program)?;
    let libfunc_name = {
        let libfunc = registry.get_libfunc(libfunc_id)?;
        libfunc_to_name(libfunc).to_string()
    };
    let LibfuncProfileData {
        deltas,
        extra_counts,
    } = data;

    // if no deltas were registered, we only return the libfunc's calls amount
    if deltas.is_empty() {
        return Ok(LibfuncProfileSummary {
            libfunc_name,
            samples: *extra_counts,
            total_time: None,
            average_time: None,
            std_deviation: None,
            quartiles: None,
        });
    }

    deltas.sort();

    // Drop outliers.
    {
        let q1 = deltas[deltas.len() / 4];
        let q3 = deltas[3 * deltas.len() / 4];
        let iqr = q3 - q1;

        let q1_thr = q1.saturating_sub(iqr + iqr / 2);
        let q3_thr = q3 + (iqr + iqr / 2);

        deltas.retain(|x| *x >= q1_thr && *x <= q3_thr);
    }

    // Compute the quartiles.
    let quartiles = [
        deltas.first().copied().unwrap(),
        deltas[deltas.len() / 4],
        deltas[deltas.len() / 2],
        deltas[3 * deltas.len() / 4],
        deltas.last().copied().unwrap(),
    ];

    // Compuite the average.
    let average = deltas.iter().copied().sum::<u64>() as f64 / deltas.len() as f64;

    // Compute the standard deviation.
    let std_dev = {
        let sum = deltas
            .iter()
            .copied()
            .map(|x| x as f64)
            .map(|x| (x - average))
            .map(|x| x * x)
            .sum::<f64>();
        sum / (deltas.len() as u64 + *extra_counts) as f64
    };

    Ok(LibfuncProfileSummary {
        libfunc_name,
        samples: deltas.len() as u64 + *extra_counts,
        total_time: Some(
            deltas.iter().copied().sum::<u64>() + (*extra_counts as f64 * average).round() as u64,
        ),
        average_time: Some(average),
        std_deviation: Some(std_dev),
        quartiles: Some(quartiles),
    })
}
