use std::{fs::File, path::PathBuf};

use blockifier::execution::native::executor::LIBFUNC_PROFILES_MAP;
use cairo_lang_sierra::{
    extensions::core::{CoreLibfunc, CoreType},
    ids::ConcreteLibfuncId,
    program::Program,
    program_registry::{ProgramRegistry, ProgramRegistryError},
};
use cairo_native::{debug::libfunc_to_name, metadata::profiler::LibfuncProfileData};
use serde::Serialize;
use starknet_types_core::felt::Felt;

#[derive(Serialize)]
struct LibfuncProfile {
    block_number: u64,
    class_hash: String,
    tx: String,
    selector: Felt,
    data: Vec<LibfuncProfileSummary>,
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

pub fn create_libfunc_profile(block_number: u64, tx_hash_str: &str) {
    let mut profiles = LIBFUNC_PROFILES_MAP.lock().unwrap();
    let root = PathBuf::from(format!("libfunc_profiles/block{block_number}"));

    std::fs::create_dir_all(&root).unwrap();

    let mut path = root.join(tx_hash_str);
    path.set_extension("json");

    let profile_file = File::create(path).unwrap();

    let profiles_data = profiles
        .iter_mut()
        .map(|(entrypoint, (profile, program))| {
            // sort deltas so that we can compute quartiles
            profile.iter_mut().for_each(|(_, data)| data.deltas.sort());

            (entrypoint, process_profile(profile, program).unwrap())
        })
        .map(|((class_hash, selector), profile)| LibfuncProfile {
            block_number,
            class_hash: class_hash.to_hex_string(),
            tx: tx_hash_str.to_string(),
            selector: *selector,
            data: profile,
        })
        .collect::<Vec<_>>();

    serde_json::to_writer_pretty(profile_file, &profiles_data).unwrap();

    profiles.clear();
}

pub fn process_profile(
    profile: &[(ConcreteLibfuncId, LibfuncProfileData)],
    program: &Program,
) -> Result<Vec<LibfuncProfileSummary>, Box<ProgramRegistryError>> {
    let registry: ProgramRegistry<CoreType, CoreLibfunc> = ProgramRegistry::new(program)?;

    let processed_profile = profile
        .iter()
        .map(
            |(
                libfunc_idx,
                LibfuncProfileData {
                    deltas,
                    extra_counts,
                },
            )| {
                let libfunc_name = {
                    let libfunc = registry.get_libfunc(libfunc_idx)?;
                    libfunc_to_name(libfunc).to_string()
                };

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

                // Drop outliers.
                let deltas = {
                    let q1 = deltas[deltas.len() / 4];
                    let q3 = deltas[3 * deltas.len() / 4];
                    let iqr = q3 - q1;

                    let q1_thr = q1.saturating_sub(iqr + iqr / 2);
                    let q3_thr = q3 + (iqr + iqr / 2);

                    deltas
                        .iter()
                        .filter(|x| **x >= q1_thr && **x <= q3_thr)
                        .collect::<Vec<_>>()
                };

                // Compute the quartiles.
                let quartiles = [
                    *deltas.first().copied().unwrap(),
                    *deltas[deltas.len() / 4],
                    *deltas[deltas.len() / 2],
                    *deltas[3 * deltas.len() / 4],
                    *deltas.last().copied().unwrap(),
                ];

                // Compuite the average.
                let average = deltas.iter().copied().sum::<u64>() as f64 / deltas.len() as f64;

                // Compute the standard deviation.
                let std_dev = {
                    let sum = deltas
                        .iter()
                        .copied()
                        .map(|x| *x as f64)
                        .map(|x| (x - average))
                        .map(|x| x * x)
                        .sum::<f64>();
                    sum / (deltas.len() as u64 + *extra_counts) as f64
                };

                Ok(LibfuncProfileSummary {
                    libfunc_name,
                    samples: deltas.len() as u64 + *extra_counts,
                    total_time: Some(
                        deltas.into_iter().sum::<u64>()
                            + (*extra_counts as f64 * average).round() as u64,
                    ),
                    average_time: Some(average),
                    std_deviation: Some(std_dev),
                    quartiles: Some(quartiles),
                })
            },
        )
        .collect::<Result<Vec<_>, Box<ProgramRegistryError>>>()?;

    Ok(processed_profile)
}
