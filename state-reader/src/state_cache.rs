use std::{collections::HashMap, fs::File, io::Write, thread, time::Duration};

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

use crate::objects::RpcTransactionReceipt;

#[serde_as]
#[derive(Serialize, Deserialize)]
pub struct RemoteCache {
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

impl RemoteCache {
    pub fn new() -> Self {
        let cache_path = format!("cache/rpc.json");
        let lockfile_path = format!("{}.lock", cache_path);

        let mut lockfile = Lockfile::create_with_parents(&lockfile_path);
        while let Err(lockfile::Error::LockTaken) = lockfile {
            thread::sleep(Duration::from_secs(1));
            lockfile = Lockfile::create_with_parents(&lockfile_path);
        }
        let lockfile = lockfile.expect("failed to take lock");

        let cache = match File::open(cache_path) {
            Ok(file) => serde_json::from_reader(file).expect("failed to read cache"),
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

        lockfile.release().expect("failed to release lockfile");

        cache
    }

    pub fn save(&mut self) {
        let cache_path = format!("cache/rpc.json");

        let lockfile_path = format!("{}.lock", cache_path);

        let mut lockfile = Lockfile::create_with_parents(&lockfile_path);
        while let Err(lockfile::Error::LockTaken) = lockfile {
            thread::sleep(Duration::from_secs(1));
            lockfile = Lockfile::create_with_parents(&lockfile_path);
        }
        let lockfile = lockfile.expect("failed to take lock");

        if let Ok(file) = File::open(&cache_path) {
            let _existing_cache: RemoteCache =
                serde_json::from_reader(file).expect("failed to read cache");
        }

        let mut file = File::create(&cache_path).expect("failed to create cache file");

        serde_json::to_writer(&file, &self).expect("failed to write cache file");

        file.flush().expect("failed to flush file");

        lockfile.release().expect("failed to release lockfile");
    }
}
