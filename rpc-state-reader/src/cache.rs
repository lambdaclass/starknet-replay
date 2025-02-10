use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    fs::{self, File},
    io::{Seek, Write},
    path::PathBuf,
};

use blockifier::state::state_api::{StateReader as BlockifierStateReader, StateResult};
use cairo_vm::Felt252;
use flate2::Compression;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet::core::types::ContractClass;
use starknet_api::{
    core::{ChainId, ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};
use tracing::warn;

use crate::{
    objects::{BlockWithTxHahes, RpcTransactionReceipt, RpcTransactionTrace},
    reader::{compile_contract_class, RpcStateReader, StateReader},
};

/// The RpcCache stores the result of RPC calls to memory (and disk)
#[serde_as]
#[derive(Default, Serialize, Deserialize)]
pub struct RpcCache {
    pub block: Option<BlockWithTxHahes>,
    // we need to serialize it as a vector to allow non string key types
    #[serde_as(as = "Vec<(_, _)>")]
    pub transactions: HashMap<TransactionHash, Transaction>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub contract_classes: HashMap<ClassHash, ContractClass>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub storage: HashMap<(ContractAddress, StorageKey), Felt252>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub nonces: HashMap<ContractAddress, Nonce>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub class_hashes: HashMap<ContractAddress, ClassHash>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub transaction_receipts: HashMap<TransactionHash, RpcTransactionReceipt>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub transaction_traces: HashMap<TransactionHash, RpcTransactionTrace>,
}

/// A wrapper around `RpcStateReader` that caches all rpc calls.
///
/// On drop, the cache is saved to disk at `rpc_cache/{block_number}.json`.
/// It's safe to use multiple instances of this struct at the same time,
/// as there is a mechanism for file locking.
pub struct RpcCachedStateReader {
    pub reader: RpcStateReader,
    state: RefCell<RpcCache>,
}

impl Drop for RpcCachedStateReader {
    fn drop(&mut self) {
        let path = PathBuf::from(format!("rpc_cache/{}.json.gz", self.reader.block_number));
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();

        let mut file = File::options()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)
            .unwrap();
        file.lock_exclusive().unwrap();

        // try to read old cache, and merge it with the current one
        let mut reader = flate2::read::GzDecoder::new(&file);
        if let Ok(old_state) = serde_json::from_reader::<_, RpcCache>(&mut reader) {
            merge_cache(self.state.get_mut(), old_state);
        }

        // overwrite the file with the new cache
        file.set_len(0).unwrap();
        file.seek(std::io::SeekFrom::Start(0)).unwrap();

        let mut writer = flate2::write::GzEncoder::new(&file, Compression::best());
        serde_json::to_writer(&mut writer, &self.state).unwrap();
        writer.finish().unwrap();

        file.flush().unwrap();

        fs2::FileExt::unlock(&file).unwrap();
    }
}

impl RpcCachedStateReader {
    pub fn new(reader: RpcStateReader) -> Self {
        let state = {
            let path = PathBuf::from(format!("rpc_cache/{}.json.gz", reader.block_number));

            match File::open(path) {
                Ok(file) => {
                    fs2::FileExt::lock_shared(&file).unwrap();
                    let mut reader = flate2::read::GzDecoder::new(&file);
                    let state = serde_json::from_reader(&mut reader).unwrap();
                    fs2::FileExt::unlock(&file).unwrap();
                    state
                }
                Err(_) => {
                    warn!("Failed to read cache for block {}", reader.block_number);
                    RpcCache::default()
                }
            }
        };

        Self {
            reader,
            state: RefCell::new(state),
        }
    }
}

impl StateReader for RpcCachedStateReader {
    fn get_block_with_tx_hashes(&self) -> StateResult<BlockWithTxHahes> {
        if let Some(block) = &self.state.borrow().block {
            return Ok(block.clone());
        }

        let result = self.reader.get_block_with_tx_hashes()?;

        self.state.borrow_mut().block = Some(result.clone());

        Ok(result)
    }

    fn get_transaction(&self, hash: &TransactionHash) -> StateResult<Transaction> {
        Ok(match self.state.borrow_mut().transactions.entry(*hash) {
            Entry::Occupied(occupied_entry) => occupied_entry.get().clone(),
            Entry::Vacant(vacant_entry) => {
                let result = self.reader.get_transaction(hash)?;
                vacant_entry.insert(result.clone());
                result
            }
        })
    }

    fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<ContractClass> {
        Ok(
            match self.state.borrow_mut().contract_classes.entry(*class_hash) {
                Entry::Occupied(occupied_entry) => occupied_entry.get().clone(),
                Entry::Vacant(vacant_entry) => {
                    let result = self.reader.get_contract_class(class_hash)?;
                    vacant_entry.insert(result.clone());
                    result
                }
            },
        )
    }

    fn get_transaction_trace(&self, hash: &TransactionHash) -> StateResult<RpcTransactionTrace> {
        Ok(
            match self.state.borrow_mut().transaction_traces.entry(*hash) {
                Entry::Occupied(occupied_entry) => occupied_entry.get().clone(),
                Entry::Vacant(vacant_entry) => {
                    let result = self.reader.get_transaction_trace(hash)?;
                    vacant_entry.insert(result.clone());
                    result
                }
            },
        )
    }

    fn get_transaction_receipt(
        &self,
        hash: &TransactionHash,
    ) -> StateResult<RpcTransactionReceipt> {
        Ok(
            match self.state.borrow_mut().transaction_receipts.entry(*hash) {
                Entry::Occupied(occupied_entry) => occupied_entry.get().clone(),
                Entry::Vacant(vacant_entry) => {
                    let result = self.reader.get_transaction_receipt(hash)?;
                    vacant_entry.insert(result.clone());
                    result
                }
            },
        )
    }

    fn get_chain_id(&self) -> ChainId {
        self.reader.get_chain_id()
    }
}

impl BlockifierStateReader for RpcCachedStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt252> {
        Ok(
            match self
                .state
                .borrow_mut()
                .storage
                .entry((contract_address, key))
            {
                Entry::Occupied(occupied_entry) => *occupied_entry.get(),
                Entry::Vacant(vacant_entry) => {
                    let result = self.reader.get_storage_at(contract_address, key)?;
                    vacant_entry.insert(result);
                    result
                }
            },
        )
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        Ok(
            match self.state.borrow_mut().nonces.entry(contract_address) {
                Entry::Occupied(occupied_entry) => *occupied_entry.get(),
                Entry::Vacant(vacant_entry) => {
                    let result = self.reader.get_nonce_at(contract_address)?;
                    vacant_entry.insert(result);
                    result
                }
            },
        )
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        Ok(
            match self.state.borrow_mut().class_hashes.entry(contract_address) {
                Entry::Occupied(occupied_entry) => *occupied_entry.get(),
                Entry::Vacant(vacant_entry) => {
                    let result = self.reader.get_class_hash_at(contract_address)?;
                    vacant_entry.insert(result);
                    result
                }
            },
        )
    }

    fn get_compiled_class(
        &self,
        class_hash: ClassHash,
    ) -> StateResult<blockifier::execution::contract_class::RunnableCompiledClass> {
        let class = self.get_contract_class(&class_hash)?;
        Ok(compile_contract_class(class, class_hash))
    }

    fn get_compiled_class_hash(
        &self,
        _class_hash: ClassHash,
    ) -> StateResult<starknet_api::core::CompiledClassHash> {
        todo!();
    }
}

fn merge_cache(cache: &mut RpcCache, other: RpcCache) {
    if cache.block.is_none() {
        cache.block = other.block
    }
    cache.transactions.extend(other.transactions);
    cache.contract_classes.extend(other.contract_classes);
    cache.storage.extend(other.storage);
    cache.nonces.extend(other.nonces);
    cache.class_hashes.extend(other.class_hashes);
    cache
        .transaction_receipts
        .extend(other.transaction_receipts);
    cache.transaction_traces.extend(other.transaction_traces);
}
