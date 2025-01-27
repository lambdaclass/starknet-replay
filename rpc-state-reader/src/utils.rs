use std::{
    collections::HashMap,
    error::Error,
    fs::{self, File},
    io::{self, Read},
    path::{Path, PathBuf},
    sync::{OnceLock, RwLock},
    time::Instant,
};

use blockifier::{
    execution::{call_info::CallInfo, contract_class::CompiledClassV1},
    transaction::objects::TransactionExecutionInfo,
};
use cairo_lang_starknet_classes::contract_class::{ContractClass, ContractEntryPoints};
use cairo_lang_utils::bigint::BigUintAsHex;
use cairo_native::{executor::AotContractExecutor, OptLevel};
use serde::{Deserialize, Serialize};
use starknet::core::types::{LegacyContractEntryPoint, LegacyEntryPointsByType};
use starknet_api::{
    block::BlockNumber,
    contract_class::EntryPointType,
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

#[derive(Debug, Serialize)]
struct EntryPointExecution {
    class_hash: ClassHash,
    selector: EntryPointSelector,
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

            executor
        }
    }
}

pub fn get_casm_compiled_class(class: ContractClass, _class_hash: ClassHash) -> CompiledClassV1 {
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

    CompiledClassV1::try_from(casm_class).unwrap()
}

pub fn bytecode_size(data: &[BigUintAsHex]) -> usize {
    data.iter().map(|n| n.value.to_bytes_be().len()).sum()
}

pub fn save_entry_point_execution(
    path: &Path,
    executions: Vec<(u64, TransactionExecutionInfo)>,
) -> Result<(), Box<dyn Error>> {
    let mut block_executions: HashMap<u64, Vec<HashMap<String, _>>> = HashMap::new();

    for (block_number, execution) in executions {
        let mut tx_execution: HashMap<String, _> = HashMap::new();

        if let Some(call) = execution.validate_call_info {
            tx_execution.insert(
                "validate_call_info".to_string(),
                get_inner_class_executions(call),
            );
        }
        if let Some(call) = execution.execute_call_info {
            tx_execution.insert(
                "execute_call_info".to_string(),
                get_inner_class_executions(call),
            );
        }
        if let Some(call) = execution.fee_transfer_call_info {
            tx_execution.insert(
                "fee_transfer_call_info".to_string(),
                get_inner_class_executions(call),
            );
        }

        block_executions
            .entry(block_number)
            .or_insert_with(Vec::new)
            .push(tx_execution);
    }

    let file = File::create(path)?;
    serde_json::to_writer_pretty(file, &block_executions)?;

    Ok(())
}

fn get_inner_class_executions(call: CallInfo) -> Vec<EntryPointExecution> {
    // class hash can initially be None, but it is always added before execution
    let class_hash = call.call.class_hash.unwrap();

    let mut classes = call
        .inner_calls
        .into_iter()
        .flat_map(|call| get_inner_class_executions(call))
        .collect::<Vec<_>>();

    if call.time.is_zero() {
        panic!("contract time should never be zero, there is a bug somewhere")
    }

    let top_class = EntryPointExecution {
        class_hash,
        selector: call.call.entry_point_selector,
    };

    classes.push(top_class);

    classes
}
