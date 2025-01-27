use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Read, Write},
    path::PathBuf,
    sync::{OnceLock, RwLock},
    time::Instant,
};

use blockifier::execution::contract_class::CompiledClassV1;
use cairo_lang_starknet_classes::contract_class::{ContractClass, ContractEntryPoints};
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

            let executor = if path.exists() {
                AotContractExecutor::load(&path).unwrap()
            } else {
                info!("starting native contract compilation");

                let pre_compilation_instant = Instant::now();
                let mut executor = AotContractExecutor::new(
                    &contract.extract_sierra_program().unwrap(),
                    &contract.entry_points_by_type,
                    OptLevel::Aggressive,
                )
                .unwrap();
                let compilation_time = pre_compilation_instant.elapsed().as_millis();

                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                executor.save(&path).unwrap();

                let library_size = fs::metadata(path).unwrap().len();

                info!(
                    time = compilation_time,
                    size = library_size,
                    "native contract compilation finished"
                );

                executor
            };

            cache.insert(class_hash, executor.clone());

            {
                let path = PathBuf::from(format!(
                    "compiled_programs/{}.sierra",
                    class_hash.to_hex_string()
                ));
                let program = contract.extract_sierra_program().unwrap();
                let mut file = File::create(path).unwrap();
                write!(file, "{}", program).unwrap();
            }

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
