use std::{
    collections::HashMap,
    fs::{self, File},
    io::{self, Write},
    thread,
    time::Duration,
};

use lockfile::Lockfile;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet_api::{
    block::BlockNumber,
    core::{ChainId, ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};
use starknet_core::types::{BlockWithTxHashes, ContractClass};
use starknet_types_core::felt::Felt;
use thiserror::Error;

use crate::objects::RpcTransactionReceipt;

#[derive(Debug, Error)]
pub enum StateCacheError {
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    LockfileError(#[from] lockfile::Error),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
}

/// A Cache for network state. Its saved to disk as is.
///
/// TODO: Separate between networks.
///
/// TODO: This cache is saved to disk as is. This implies that the file can get
/// big fast (2GB for 400k transactions). Although the size cannot be reduced
/// easily, we can increase the loading times by separating them into different
/// files:
/// - 1 file for each block
/// - 1 file for each contract class
#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct StateCache {
    pub chain_id: Option<ChainId>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub blocks: HashMap<BlockNumber, BlockWithTxHashes>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub transactions: HashMap<TransactionHash, Transaction>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub transaction_receipts: HashMap<TransactionHash, RpcTransactionReceipt>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub contract_classes: HashMap<ClassHash, ContractClass>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub nonces: HashMap<(BlockNumber, ContractAddress), Nonce>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub class_hashes: HashMap<(BlockNumber, ContractAddress), ClassHash>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub storage: HashMap<(BlockNumber, ContractAddress, StorageKey), Felt>,
}

impl Default for StateCache {
    fn default() -> Self {
        Self::new()
    }
}

impl StateCache {
    pub fn new() -> Self {
        Self {
            blocks: Default::default(),
            transactions: Default::default(),
            transaction_receipts: Default::default(),
            contract_classes: Default::default(),
            nonces: Default::default(),
            class_hashes: Default::default(),
            storage: Default::default(),
            chain_id: Default::default(),
        }
    }

    pub fn load() -> Result<Self, StateCacheError> {
        let cache_path = "cache/rpc.json".to_string();
        let lockfile_path = format!("{}.lock", cache_path);

        let mut lockfile = Lockfile::create_with_parents(&lockfile_path);
        while let Err(lockfile::Error::LockTaken) = lockfile {
            thread::sleep(Duration::from_secs(1));
            lockfile = Lockfile::create_with_parents(&lockfile_path);
        }
        let lockfile = lockfile?;

        let default_cache = Self {
            blocks: Default::default(),
            transactions: Default::default(),
            transaction_receipts: Default::default(),
            contract_classes: Default::default(),
            nonces: Default::default(),
            class_hashes: Default::default(),
            storage: Default::default(),
            chain_id: Default::default(),
        };

        let cache = match File::open(cache_path) {
            Ok(file) => serde_json::from_reader(file).unwrap_or(default_cache),
            Err(_) => Self {
                blocks: Default::default(),
                transactions: Default::default(),
                transaction_receipts: Default::default(),
                contract_classes: Default::default(),
                nonces: Default::default(),
                class_hashes: Default::default(),
                storage: Default::default(),
                chain_id: Default::default(),
            },
        };

        lockfile.release()?;

        Ok(cache)
    }

    pub fn merge(&mut self, other: StateCache) {
        if other.chain_id.is_some() {
            if self.chain_id.is_some() {
                assert_eq!(other.chain_id, self.chain_id)
            } else {
                self.chain_id = other.chain_id;
            }
        }
        other.blocks.into_iter().for_each(|(k, v)| {
            let old = self.blocks.insert(k, v.clone());
            if let Some(old) = old {
                assert_eq!(old, v)
            }
        });
        other.transactions.into_iter().for_each(|(k, v)| {
            let old = self.transactions.insert(k, v.clone());
            if let Some(old) = old {
                assert_eq!(old, v)
            }
        });
        other.transaction_receipts.into_iter().for_each(|(k, v)| {
            let old = self.transaction_receipts.insert(k, v.clone());
            if let Some(old) = old {
                assert_eq!(old, v)
            }
        });
        other.contract_classes.into_iter().for_each(|(k, v)| {
            let old = self.contract_classes.insert(k, v.clone());
            if let Some(old) = old {
                assert_eq!(old, v)
            }
        });
        other.nonces.into_iter().for_each(|(k, v)| {
            let old = self.nonces.insert(k, v);
            if let Some(old) = old {
                assert_eq!(old, v)
            }
        });
        other.class_hashes.into_iter().for_each(|(k, v)| {
            let old = self.class_hashes.insert(k, v);
            if let Some(old) = old {
                assert_eq!(old, v)
            }
        });
        other.storage.into_iter().for_each(|(k, v)| {
            let old = self.storage.insert(k, v);
            if let Some(old) = old {
                assert_eq!(old, v)
            }
        });
    }

    pub fn save(&mut self) -> Result<(), StateCacheError> {
        let cache_path = "cache/rpc.json".to_string();
        let tmp_path = format!("{}.tmp", cache_path);
        let lockfile_path = format!("{}.lock", cache_path);

        let mut lockfile = Lockfile::create_with_parents(&lockfile_path);
        while let Err(lockfile::Error::LockTaken) = lockfile {
            thread::sleep(Duration::from_secs(1));
            lockfile = Lockfile::create_with_parents(&lockfile_path);
        }
        let lockfile = lockfile?;

        if let Ok(file) = File::open(&cache_path) {
            if let Ok(existing_cache) = serde_json::from_reader(file) {
                self.merge(existing_cache);
            }
        }

        // Use temporary file and rename to final path. As the rename syscall is
        // atomic, we ensure that the cache file is never invalid.
        let mut file = File::create(&tmp_path)?;
        serde_json::to_writer(&file, &self)?;
        file.flush()?;
        drop(file);
        fs::rename(tmp_path, cache_path)?;

        lockfile.release()?;

        Ok(())
    }
}
