use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Read},
    path::PathBuf,
    thread::{self, sleep},
    time::Duration,
};

use blockifier::execution::{
    contract_class::{CompiledClassV0, CompiledClassV1, RunnableCompiledClass},
    native::contract_class::NativeCompiledClassV1,
};
use cairo_native::{executor::AotContractExecutor, statistics::Statistics, OptLevel};
use cairo_vm::types::errors::program_errors::ProgramError;
use lockfile::Lockfile;
use starknet_api::{
    contract_class::{
        ClassInfo, ContractClass as CompiledContractClass, EntryPointType, SierraVersion,
        VersionedCasm,
    },
    core::{ClassHash, EntryPointSelector},
    deprecated_contract_class::{
        ContractClass as DeprecatedContractClass, EntryPointOffset, EntryPointV0,
    },
    hash::StarkHash,
};
use starknet_core::types::{
    CompressedLegacyContractClass, ContractClass as ApiContractClass, FlattenedSierraClass,
    LegacyContractEntryPoint, LegacyEntryPointsByType,
};
use thiserror::Error;

use cairo_lang_starknet_classes::{
    casm_contract_class::{CasmContractClass, StarknetSierraCompilationError},
    contract_class::{version_id_from_serialized_sierra_program, ContractClass},
};

#[derive(Error, Debug)]
pub enum ClassManagerError {
    #[error(transparent)]
    CairoNativeError(#[from] cairo_native::error::Error),
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    ProgramError(#[from] ProgramError),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
    #[error(transparent)]
    StarknetApiError(#[from] starknet_api::StarknetApiError),
    #[error(transparent)]
    StarknetSierraCompilationError(#[from] StarknetSierraCompilationError),
    #[error("a legacy contract should always have an ABI")]
    LegacyContractWithoutAbi,
}

#[derive(Default)]
pub struct ClassManager {
    runnable_classes: HashMap<ClassHash, RunnableCompiledClass>,
}

impl ClassManager {
    pub fn new() -> Self {
        Default::default()
    }

    pub fn get_runnable_class(&self, class_hash: &ClassHash) -> Option<RunnableCompiledClass> {
        self.runnable_classes.get(class_hash).cloned()
    }

    pub fn compile_runnable_class(
        &mut self,
        class_hash: &ClassHash,
        contract_class: ApiContractClass,
    ) -> Result<RunnableCompiledClass, ClassManagerError> {
        let runnable_compiled_class = match contract_class {
            ApiContractClass::Sierra(sierra_class) => {
                let casm_class = CompiledClassV1::try_from(
                    self.compile_casm_v1_class(class_hash, &sierra_class)?,
                )?;

                if cfg!(feature = "only-casm") {
                    RunnableCompiledClass::V1(casm_class)
                } else {
                    let native_executor = self.compile_native_class(class_hash, &sierra_class)?;

                    RunnableCompiledClass::V1Native(NativeCompiledClassV1::new(
                        native_executor.into(),
                        casm_class,
                    ))
                }
            }
            ApiContractClass::Legacy(legacy_class) => {
                let contract_class = decompress_v0_class(legacy_class)?;
                RunnableCompiledClass::V0(CompiledClassV0::try_from(contract_class)?)
            }
        };

        self.runnable_classes
            .insert(*class_hash, runnable_compiled_class.clone());

        Ok(runnable_compiled_class)
    }

    pub fn compile_casm_v1_class(
        &self,
        class_hash: &ClassHash,
        sierra_class: &FlattenedSierraClass,
    ) -> Result<VersionedCasm, ClassManagerError> {
        let contract_class = processed_class_to_contract_class(sierra_class)?;

        let cache_path = format!("cache/casm/{}.json", class_hash.to_hex_string());
        let lockfile_path = format!("{}.lock", cache_path);

        let mut lockfile = Lockfile::create_with_parents(&lockfile_path);
        while let Err(lockfile::Error::LockTaken) = lockfile {
            thread::sleep(Duration::from_secs(1));
            lockfile = Lockfile::create_with_parents(&lockfile_path);
        }
        let lockfile = lockfile.expect("failed to take lock");

        let versioned_casm_class = match File::open(&cache_path) {
            Ok(file) => serde_json::from_reader(file)?,
            Err(_) => {
                let sierra_program_values = contract_class
                    .sierra_program
                    .iter()
                    .take(3)
                    .map(|felt| felt.value.clone())
                    .collect::<Vec<_>>();

                let sierra_version = SierraVersion::extract_from_program(&sierra_program_values)?;

                let casm_class =
                    CasmContractClass::from_contract_class(contract_class, false, usize::MAX)?;

                let versioned_casm_class = (casm_class, sierra_version);

                let file = File::create(&cache_path)?;
                serde_json::to_writer(file, &versioned_casm_class)?;

                versioned_casm_class
            }
        };

        lockfile.release().expect("failed to release lockfile");

        Ok(versioned_casm_class)
    }

    pub fn compile_native_class(
        &self,
        class_hash: &ClassHash,
        sierra_class: &FlattenedSierraClass,
    ) -> Result<AotContractExecutor, ClassManagerError> {
        let contract_class = processed_class_to_contract_class(sierra_class)?;

        let cache_path =
            PathBuf::from(format!("cache/native/{}.{}", class_hash.to_hex_string(), {
                if cfg!(target_os = "macos") {
                    "dylib"
                } else {
                    "so"
                }
            }));

        if let Some(p) = cache_path.parent() {
            fs::create_dir_all(p)?;
        }

        let (sierra_version, _) =
            version_id_from_serialized_sierra_program(&contract_class.sierra_program)
                .map_err(StarknetSierraCompilationError::from)?;

        let native_executor = loop {
            // it could be the case that the file was created after we've entered this branch
            // so we should load it instead of compiling it again
            if cache_path.exists() {
                match AotContractExecutor::from_path(&cache_path)? {
                    None => {
                        sleep(Duration::from_secs(1));
                        continue;
                    }
                    Some(e) => break e,
                }
            }

            let mut statistics = if cfg!(feature = "with-comp-stats") {
                Some(Statistics::default())
            } else {
                None
            };

            match AotContractExecutor::new_into(
                &contract_class
                    .extract_sierra_program()
                    .map_err(StarknetSierraCompilationError::from)?,
                &contract_class.entry_points_by_type,
                sierra_version,
                &cache_path,
                OptLevel::Aggressive,
                statistics.as_mut(),
            )? {
                Some(e) => {
                    if let Some(statistics) = statistics {
                        let stats_path = cache_path.with_extension("stats.json");
                        let stats_file = File::create(stats_path)?;
                        serde_json::to_writer_pretty(stats_file, &statistics)?;
                    }

                    break e;
                }
                None => {
                    sleep(Duration::from_secs(1));
                    continue;
                }
            }
        };

        Ok(native_executor)
    }

    pub fn get_class_info(
        &self,
        class_hash: &ClassHash,
        contract_class: ApiContractClass,
    ) -> Result<ClassInfo, ClassManagerError> {
        Ok(match contract_class {
            ApiContractClass::Legacy(legacy_class) => {
                let abi_length = legacy_class
                    .abi
                    .as_ref()
                    .ok_or(ClassManagerError::LegacyContractWithoutAbi)?
                    .len();

                let casm_class = decompress_v0_class(legacy_class)?;

                ClassInfo::new(
                    &CompiledContractClass::V0(casm_class),
                    0,
                    abi_length,
                    SierraVersion::DEPRECATED,
                )?
            }
            ApiContractClass::Sierra(sierra_class) => {
                let abi_length = sierra_class.abi.len();
                let sierra_length = sierra_class.sierra_program.len();

                let versioned_casm = self.compile_casm_v1_class(class_hash, &sierra_class)?;
                let sierra_version = versioned_casm.1.clone();

                ClassInfo::new(
                    &CompiledContractClass::V1(versioned_casm),
                    sierra_length,
                    abi_length,
                    sierra_version,
                )?
            }
        })
    }
}

pub fn decompress_v0_class(
    class: CompressedLegacyContractClass,
) -> Result<DeprecatedContractClass, ClassManagerError> {
    let program_as_string = gz_decode_bytes_into_string(&class.program)?;
    let program = serde_json::from_str(&program_as_string)?;
    let entry_points_by_type = map_legacy_entrypoints_by_type(class.entry_points_by_type);

    Ok(DeprecatedContractClass {
        abi: None,
        program,
        entry_points_by_type,
    })
}

pub fn compile_v1_class(class: ContractClass) -> Result<VersionedCasm, ClassManagerError> {
    let sierra_program_values = class
        .sierra_program
        .iter()
        .take(3)
        .map(|felt| felt.value.clone())
        .collect::<Vec<_>>();

    let sierra_version = SierraVersion::extract_from_program(&sierra_program_values)?;

    let casm_class =
        cairo_lang_starknet_classes::casm_contract_class::CasmContractClass::from_contract_class(
            class,
            false,
            usize::MAX,
        )?;

    Ok((casm_class, sierra_version))
}

/// Converts the processed class format into the compiler class format.
pub fn processed_class_to_contract_class(
    sierra_class: &FlattenedSierraClass,
) -> Result<ContractClass, ClassManagerError> {
    let mut value = serde_json::to_value(sierra_class)?;
    value
        .as_object_mut()
        .expect("should be an object")
        .remove("abi");
    Ok(serde_json::from_value(value)?)
}

/// Decodes a gz encoded byte slice, and returns a string. May fail if the byte
/// slice is encoded incorrectly, or if the output is not a valid string.
pub fn gz_decode_bytes_into_string(bytes: &[u8]) -> io::Result<String> {
    use flate2::bufread;
    let mut decoder = bufread::GzDecoder::new(bytes);
    let mut string = String::new();
    decoder.read_to_string(&mut string)?;
    Ok(string)
}

/// Builds a map with legacy entrypoints by type.
pub fn map_legacy_entrypoints_by_type(
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
