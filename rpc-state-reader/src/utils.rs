use std::{
    collections::HashMap,
    io::{self, Read},
    path::PathBuf,
    sync::{Arc, OnceLock, RwLock},
};

use cairo_lang_sierra::program::Program;
use cairo_lang_starknet_classes::contract_class::ContractEntryPoints;
use cairo_lang_utils::bigint::BigUintAsHex;
use cairo_native::{executor::AotContractExecutor, OptLevel};
use serde::Deserialize;
use starknet::core::types::{LegacyContractEntryPoint, LegacyEntryPointsByType};
use starknet_api::{
    core::{ClassHash, EntryPointSelector},
    deprecated_contract_class::{EntryPoint, EntryPointOffset, EntryPointType},
    hash::StarkHash,
};

#[derive(Debug, Deserialize)]
pub struct MiddleSierraContractClass {
    pub sierra_program: Vec<BigUintAsHex>,
    pub contract_class_version: String,
    pub entry_points_by_type: ContractEntryPoints,
}

static AOT_PROGRAM_CACHE: OnceLock<RwLock<HashMap<ClassHash, Arc<AotContractExecutor>>>> =
    OnceLock::new();

pub fn map_entry_points_by_type_legacy(
    entry_points_by_type: LegacyEntryPointsByType,
) -> HashMap<EntryPointType, Vec<EntryPoint>> {
    let entry_types_to_points = HashMap::from([
        (
            EntryPointType::Constructor,
            entry_points_by_type.constructor,
        ),
        (EntryPointType::External, entry_points_by_type.external),
        (EntryPointType::L1Handler, entry_points_by_type.l1_handler),
    ]);

    let to_contract_entry_point = |entrypoint: &LegacyContractEntryPoint| -> EntryPoint {
        let felt: StarkHash = StarkHash::from_bytes_be(&entrypoint.selector.to_bytes_be());
        EntryPoint {
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

pub fn get_native_executor(program: Program, class_hash: ClassHash) -> Arc<AotContractExecutor> {
    let cache_lock = AOT_PROGRAM_CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    let executor = cache_lock.read().unwrap().get(&class_hash).map(Arc::clone);

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

            let executor = Arc::new(if path.exists() {
                AotContractExecutor::load(&path).unwrap()
            } else {
                let mut executor = AotContractExecutor::new(&program, OptLevel::Default).unwrap();

                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                executor.save(&path).unwrap();

                executor
            });

            cache.insert(class_hash, Arc::clone(&executor));
            executor
        }
    }
}
