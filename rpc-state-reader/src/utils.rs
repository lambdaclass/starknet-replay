use std::{
    collections::HashMap,
    fs::{self},
    io::{self, Read},
    path::PathBuf,
    sync::{OnceLock, RwLock},
    thread::sleep,
    time::{Duration, Instant},
};

use blockifier::execution::contract_class::CompiledClassV1;
use cairo_lang_starknet_classes::contract_class::{
    version_id_from_serialized_sierra_program, ContractClass, ContractEntryPoints,
};
use cairo_lang_utils::bigint::BigUintAsHex;
use cairo_native::{executor::AotContractExecutor, OptLevel};
use serde::Deserialize;
use starknet::core::types::{LegacyContractEntryPoint, LegacyEntryPointsByType};
use starknet_api::{
    contract_class::{EntryPointType, SierraVersion},
    core::{ClassHash, EntryPointSelector},
    deprecated_contract_class::{EntryPointOffset, EntryPointV0},
    hash::StarkHash,
};
use tracing::info;

#[derive(Debug, Deserialize)]
pub struct MiddleSierraContractClass {
    pub sierra_program: Vec<BigUintAsHex>,
    pub contract_class_version: String,
    pub entry_points_by_type: ContractEntryPoints,
}

static AOT_PROGRAM_CACHE: OnceLock<RwLock<HashMap<ClassHash, AotContractExecutor>>> =
    OnceLock::new();

pub fn map_entry_points_by_type_legacy(
    entry_points_by_type: LegacyEntryPointsByType,
) -> HashMap<EntryPointType, Vec<EntryPointV0>> {
    let entry_types_to_points = HashMap::from([
        (
            EntryPointType::Constructor,
            entry_points_by_type.constructor,
        ),
        (EntryPointType::External, entry_points_by_type.external),
        (EntryPointType::L1Handler, entry_points_by_type.l1_handler),
    ]);

    let to_contract_entry_point = |entrypoint: &LegacyContractEntryPoint| -> EntryPointV0 {
        let felt: StarkHash = StarkHash::from_bytes_be(&entrypoint.selector.to_bytes_be());
        EntryPointV0 {
            offset: EntryPointOffset(entrypoint.offset as usize),
            selector: EntryPointSelector(felt),
        }
    };

    let mut entry_points_by_type_map = HashMap::new();
    for (entry_point_type, entry_points) in entry_types_to_points.into_iter() {
        let values = entry_points
            .iter()
            .map(to_contract_entry_point)
            .collect::<Vec<_>>();
        entry_points_by_type_map.insert(entry_point_type, values);
    }

    entry_points_by_type_map
}

/// Uncompresses a Gz Encoded vector of bytes and returns a string or error
/// Here &[u8] implements BufRead
pub fn decode_reader(bytes: Vec<u8>) -> io::Result<String> {
    use flate2::bufread;
    let mut gz = bufread::GzDecoder::new(&bytes[..]);
    let mut s = String::new();
    gz.read_to_string(&mut s)?;
    Ok(s)
}

pub fn get_native_executor(contract: &ContractClass, class_hash: ClassHash) -> AotContractExecutor {
    let cache_lock = AOT_PROGRAM_CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    let executor = cache_lock.read().unwrap().get(&class_hash).cloned();

    match executor {
        Some(executor) => executor,
        None => {
            let mut cache = cache_lock.write().unwrap();
            let path = PathBuf::from(format!(
                "compiled_programs/{}.{}",
                class_hash.to_hex_string(),
                {
                    if cfg!(target_os = "macos") {
                        "dylib"
                    } else {
                        "so"
                    }
                }
            ));

            if let Some(p) = path.parent() {
                let _ = fs::create_dir_all(p);
            }

            let executor = if path.exists() {
                loop {
                    match AotContractExecutor::from_path(&path).unwrap() {
                        None => sleep(Duration::from_secs(1)),
                        Some(e) => break e,
                    }
                }
            } else {
                info!("starting native contract compilation");

                let (sierra_version, _) =
                    version_id_from_serialized_sierra_program(&contract.sierra_program).unwrap();
                
                loop {
                    // it could be the case that file was created after we've entered this branch
                    // so we should load it instead of compiling it again
                    if path.exists() {
                        match AotContractExecutor::from_path(&path).unwrap() {
                            None => {
                                sleep(Duration::from_secs(1));
                                continue;
                            }
                            Some(e) => break e,
                        }
                    }

                    let pre_compilation_instant = Instant::now();

                    match AotContractExecutor::new_into(
                        &contract.extract_sierra_program().unwrap(),
                        &contract.entry_points_by_type,
                        sierra_version,
                        &path,
                        OptLevel::Aggressive,
                    )
                    .unwrap()
                    {
                        Some(e) => {
                            let library_size = fs::metadata(path).unwrap().len();

                            info!(
                                time = pre_compilation_instant.elapsed().as_millis(),
                                size = library_size,
                                "native contract compilation finished"
                            );

                            cache.insert(class_hash, e.clone());

                            break e;
                        }
                        None => {
                            sleep(Duration::from_secs(1));
                            continue;
                        }
                    }
                }
            };

            executor
        }
    }
}

pub fn get_casm_compiled_class(class: ContractClass, _class_hash: ClassHash) -> CompiledClassV1 {
    let sierra_program_values = class
        .sierra_program
        .iter()
        .take(3)
        .map(|felt| felt.value.clone())
        .collect::<Vec<_>>();
    let sierra_version = SierraVersion::extract_from_program(&sierra_program_values).unwrap();

    info!("starting vm contract compilation");

    let pre_compilation_instant = Instant::now();

    let casm_class =
        cairo_lang_starknet_classes::casm_contract_class::CasmContractClass::from_contract_class(
            class,
            false,
            usize::MAX,
        )
        .unwrap();

    let compilation_time = pre_compilation_instant.elapsed().as_millis();

    tracing::info!(
        time = compilation_time,
        size = bytecode_size(&casm_class.bytecode),
        "vm contract compilation finished"
    );

    let versioned_casm = (casm_class, sierra_version);

    CompiledClassV1::try_from(versioned_casm).unwrap()
}

pub fn bytecode_size(data: &[BigUintAsHex]) -> usize {
    data.iter().map(|n| n.value.to_bytes_be().len()).sum()
}
