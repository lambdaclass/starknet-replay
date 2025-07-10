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
    #[error("the chain id differs from the cached one")]
    InvalidChainId,
}

/// A Cache for network state. Its saved to disk as is.
///
/// TODO: This cache is saved to disk as is. This implies that the file can get
/// big fast (2GB for 400k transactions). Although the size cannot be reduced
/// easily, we can increase the loading times by separating them into different
/// files:
/// - one file for each block
/// - one file for each contract class
///
/// TODO: To reduce disk usage, we can compress the data before writing it or
/// use a format different from JSON.
#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct StateCache {
    pub chain_id: ChainId,
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

impl StateCache {
    pub fn new(chain_id: ChainId) -> Self {
        Self {
            chain_id,
            blocks: Default::default(),
            transactions: Default::default(),
            transaction_receipts: Default::default(),
            contract_classes: Default::default(),
            nonces: Default::default(),
            class_hashes: Default::default(),
            storage: Default::default(),
        }
    }

    pub fn load(chain_id: ChainId) -> Result<Self, StateCacheError> {
        let cache_path = format!("cache/rpc-{}.json", chain_id);
        let lockfile_path = format!("{}.lock", cache_path);

        // Wait until we get the file lock for the cache.
        let mut lockfile = Lockfile::create_with_parents(&lockfile_path);
        while let Err(lockfile::Error::LockTaken) = lockfile {
            thread::sleep(Duration::from_secs(1));
            lockfile = Lockfile::create_with_parents(&lockfile_path);
        }
        let lockfile = lockfile?;

        // If the cache already exists, load it.
        let cache = match File::open(cache_path)
            .map_err(StateCacheError::from)
            .and_then(|file| {
                serde_json::from_reader::<_, StateCache>(file).map_err(StateCacheError::from)
            }) {
            Ok(cache) => {
                // The disk file chain id must match our chain id.
                if cache.chain_id != chain_id {
                    return Err(StateCacheError::InvalidChainId);
                }
                cache
            }
            Err(_) => Self {
                chain_id,
                blocks: Default::default(),
                transactions: Default::default(),
                transaction_receipts: Default::default(),
                contract_classes: Default::default(),
                nonces: Default::default(),
                class_hashes: Default::default(),
                storage: Default::default(),
            },
        };

        lockfile.release()?;

        Ok(cache)
    }

    pub fn merge(&mut self, other: StateCache) {
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
        let cache_path = format!("cache/rpc-{}.json", self.chain_id);
        let tmp_path = format!("{}.tmp", cache_path);
        let lockfile_path = format!("{}.lock", cache_path);

        // Wait until we get the file lock for the cache.
        let mut lockfile = Lockfile::create_with_parents(&lockfile_path);
        while let Err(lockfile::Error::LockTaken) = lockfile {
            thread::sleep(Duration::from_secs(1));
            lockfile = Lockfile::create_with_parents(&lockfile_path);
        }
        let lockfile = lockfile?;

        // If there is a cache file, we load it and merge it with the current data.
        if let Ok(file) = File::open(&cache_path) {
            if let Ok(existing_cache) = serde_json::from_reader::<_, StateCache>(file) {
                // The disk file chain id must match our chain id.
                if existing_cache.chain_id != self.chain_id {
                    return Err(StateCacheError::InvalidChainId);
                }
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
