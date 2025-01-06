use std::{
    cell::RefCell,
    collections::{hash_map::Entry, HashMap},
    fs::{self, File},
    path::PathBuf,
};

use blockifier::state::state_api::{StateReader, StateResult};
use cairo_vm::Felt252;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet::core::types::ContractClass;
use starknet_api::{
    core::{ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};

use crate::{
    objects::{BlockWithTxHahes, RpcTransactionReceipt, RpcTransactionTrace},
    reader::{compile_contract_class, RpcStateReader},
};

/// The RpcCache stores the result of RPC calls to memory (and disk)
///
/// Each field corresponds to a particular endpoint
#[serde_as]
#[derive(Default, Serialize, Deserialize)]
pub struct RpcCache {
    pub get_block_with_tx_hashes: Option<BlockWithTxHahes>,
    // we need to serialize it as a vector to allow non string key types
    #[serde_as(as = "Vec<(_, _)>")]
    pub get_transaction_by_hash: HashMap<TransactionHash, Transaction>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub get_contract_class: HashMap<ClassHash, ContractClass>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub get_storage_at: HashMap<(ContractAddress, StorageKey), Felt252>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub get_nonce_at: HashMap<ContractAddress, Nonce>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub get_class_hash_at: HashMap<ContractAddress, ClassHash>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub get_transaction_receipt: HashMap<TransactionHash, RpcTransactionReceipt>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub get_transaction_trace: HashMap<TransactionHash, RpcTransactionTrace>,
}

/// A wrapper around `RpcStateReader` that caches all rpc calls.
///
/// On drop, the cache is saved to disk at `rpc_cache/{block_number}.json`.
/// It's not safe to use multiple instances of this struct at the same time,
/// as there is no mechanism for file locking.
pub struct RpcCachedStateReader {
    pub reader: RpcStateReader,
    state: RefCell<RpcCache>,
}

impl Drop for RpcCachedStateReader {
    fn drop(&mut self) {
        let path = PathBuf::from(format!("rpc_cache/{}.json", self.reader.block_number));
        let parent = path.parent().unwrap();
        fs::create_dir_all(parent).unwrap();
        let file = File::create(path).unwrap();
        serde_json::to_writer_pretty(file, &self.state).unwrap();
    }
}

impl RpcCachedStateReader {
    pub fn new(reader: RpcStateReader) -> Self {
        let state = {
            let path = PathBuf::from(format!("rpc_cache/{}.json", reader.block_number));

            match File::open(path) {
                Ok(file) => serde_json::from_reader(file).unwrap(),
                Err(_) => RpcCache::default(),
            }
        };

        Self {
            reader,
            state: RefCell::new(state),
        }
    }

    pub fn get_block_with_tx_hashes(&self) -> StateResult<BlockWithTxHahes> {
        if let Some(block) = &self.state.borrow().get_block_with_tx_hashes {
            return Ok(block.clone());
        }

        let result = self.reader.get_block_with_tx_hashes()?;

        self.state.borrow_mut().get_block_with_tx_hashes = Some(result.clone());

        Ok(result)
    }

    pub fn get_transaction(&self, hash: &TransactionHash) -> StateResult<Transaction> {
        Ok(
            match self.state.borrow_mut().get_transaction_by_hash.entry(*hash) {
                Entry::Occupied(occupied_entry) => occupied_entry.get().clone(),
                Entry::Vacant(vacant_entry) => {
                    let result = self.reader.get_transaction(hash)?;
                    vacant_entry.insert(result.clone());
                    result
                }
            },
        )
    }

    pub fn get_contract_class(&self, class_hash: &ClassHash) -> StateResult<ContractClass> {
        Ok(
            match self
                .state
                .borrow_mut()
                .get_contract_class
                .entry(*class_hash)
            {
                Entry::Occupied(occupied_entry) => occupied_entry.get().clone(),
                Entry::Vacant(vacant_entry) => {
                    let result = self.reader.get_contract_class(class_hash)?;
                    vacant_entry.insert(result.clone());
                    result
                }
            },
        )
    }

    pub fn get_transaction_trace(
        &self,
        hash: &TransactionHash,
    ) -> StateResult<RpcTransactionTrace> {
        Ok(
            match self.state.borrow_mut().get_transaction_trace.entry(*hash) {
                Entry::Occupied(occupied_entry) => occupied_entry.get().clone(),
                Entry::Vacant(vacant_entry) => {
                    let result = self.reader.get_transaction_trace(hash)?;
                    vacant_entry.insert(result.clone());
                    result
                }
            },
        )
    }

    pub fn get_transaction_receipt(
        &self,
        hash: &TransactionHash,
    ) -> StateResult<RpcTransactionReceipt> {
        Ok(
            match self.state.borrow_mut().get_transaction_receipt.entry(*hash) {
                Entry::Occupied(occupied_entry) => occupied_entry.get().clone(),
                Entry::Vacant(vacant_entry) => {
                    let result = self.reader.get_transaction_receipt(hash)?;
                    vacant_entry.insert(result.clone());
                    result
                }
            },
        )
    }
}

impl StateReader for RpcCachedStateReader {
    fn get_storage_at(
        &self,
        contract_address: ContractAddress,
        key: StorageKey,
    ) -> StateResult<Felt252> {
        Ok(
            match self
                .state
                .borrow_mut()
                .get_storage_at
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
            match self.state.borrow_mut().get_nonce_at.entry(contract_address) {
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
            match self
                .state
                .borrow_mut()
                .get_class_hash_at
                .entry(contract_address)
            {
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
