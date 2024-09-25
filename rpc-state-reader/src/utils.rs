use std::{
    collections::HashMap,
    fs,
    io::{self, Read},
    path::PathBuf,
    sync::{Arc, OnceLock, RwLock},
    time::Instant,
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
    transaction::{DeclareTransaction, DeployAccountTransaction, InvokeTransaction, Transaction},
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

/// Freestanding deserialize method to avoid a new type.
pub fn deserialize_transaction_json(
    transaction: serde_json::Value,
) -> serde_json::Result<Transaction> {
    let tx_type: String = serde_json::from_value(transaction["type"].clone())?;
    let tx_version: String = serde_json::from_value(transaction["version"].clone())?;

    match tx_type.as_str() {
        "INVOKE" => match tx_version.as_str() {
            "0x0" => Ok(Transaction::Invoke(InvokeTransaction::V0(
                serde_json::from_value(transaction)?,
            ))),
            "0x1" => Ok(Transaction::Invoke(InvokeTransaction::V1(
                serde_json::from_value(transaction)?,
            ))),
            "0x3" => Ok(Transaction::Invoke(InvokeTransaction::V3(
                serde_json::from_value(transaction)?,
            ))),
            x => Err(serde::de::Error::custom(format!(
                "unimplemented invoke version: {x}"
            ))),
        },
        "DEPLOY_ACCOUNT" => match tx_version.as_str() {
            "0x1" => Ok(Transaction::DeployAccount(DeployAccountTransaction::V1(
                serde_json::from_value(transaction)?,
            ))),
            "0x3" => Ok(Transaction::DeployAccount(DeployAccountTransaction::V3(
                serde_json::from_value(transaction)?,
            ))),
            x => Err(serde::de::Error::custom(format!(
                "unimplemented declare version: {x}"
            ))),
        },
        "DECLARE" => match tx_version.as_str() {
            "0x0" => Ok(Transaction::Declare(DeclareTransaction::V0(
                serde_json::from_value(transaction)?,
            ))),
            "0x1" => Ok(Transaction::Declare(DeclareTransaction::V1(
                serde_json::from_value(transaction)?,
            ))),
            "0x2" => Ok(Transaction::Declare(DeclareTransaction::V2(
                serde_json::from_value(transaction)?,
            ))),
            "0x3" => Ok(Transaction::Declare(DeclareTransaction::V3(
                serde_json::from_value(transaction)?,
            ))),
            x => Err(serde::de::Error::custom(format!(
                "unimplemented declare version: {x}"
            ))),
        },
        "L1_HANDLER" => Ok(Transaction::L1Handler(serde_json::from_value(transaction)?)),
        x => Err(serde::de::Error::custom(format!(
            "unimplemented transaction type deserialization: {x}"
        ))),
    }
}

pub fn get_native_executor(program: Program, class_hash: ClassHash) -> Arc<AotContractExecutor> {
    let program_cache = AOT_PROGRAM_CACHE.get_or_init(|| RwLock::new(HashMap::with_capacity(32)));

    let cache = program_cache.read().unwrap();
    let native_executor = cache.get(&class_hash);

    match native_executor {
        Some(executor) => Arc::clone(executor),
        None => {
            drop(cache);
            let mut cache = program_cache.write().unwrap();

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
                let pre_compilation_instant = Instant::now();
                let mut executor = AotContractExecutor::new(&program, OptLevel::Default).unwrap();
                let compilation_time = pre_compilation_instant.elapsed().as_millis();

                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                executor.save(&path).unwrap();

                let library_size = fs::metadata(path).unwrap().len();

                tracing::info!(
                    class_hash = class_hash.to_string(),
                    time = compilation_time,
                    library_size = library_size,
                    "native compilation finished"
                );

                executor
            });

            cache.insert(class_hash, Arc::clone(&executor));
            executor
        }
    }
}
