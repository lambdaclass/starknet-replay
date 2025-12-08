//! This crate contains data types for keeping a node state in memory.
//!
//! The main structure is [StateCache](`StateCache`), which contains the whole
//! node state. This includes static information, like transactions or contract
//! classes, as well as the pre node state for each block.

use std::collections::{hash_map, HashMap};

use blockifier_reexecution::state_reader::offline_state_reader::OfflineConsecutiveStateReaders;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet_api::{
    block::BlockNumber,
    core::{ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};
use starknet_core::types::{BlockWithTxHashes, ContractClass};
use starknet_types_core::felt::Felt;

use crate::objects::RpcTransactionReceipt;

#[derive(Default)]
pub struct StateCache {
    pub blocks: HashMap<BlockNumber, BlockWithTxHashes>,
    pub transactions: HashMap<TransactionHash, Transaction>,
    pub transaction_receipts: HashMap<TransactionHash, RpcTransactionReceipt>,
    pub contract_classes: HashMap<ClassHash, ContractClass>,
    pub block_states: HashMap<BlockNumber, BlockState>,
}

#[serde_as]
#[derive(Serialize, Deserialize, Default, Clone)]
pub struct BlockState {
    #[serde_as(as = "Vec<(_, _)>")]
    pub nonces: HashMap<ContractAddress, Nonce>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub class_hashes: HashMap<ContractAddress, ClassHash>,
    #[serde_as(as = "Vec<(_, _)>")]
    pub storage: HashMap<(ContractAddress, StorageKey), Felt>,
}

pub fn merge_block_state(mut v1: BlockState, v2: BlockState) -> BlockState {
    v2.nonces.into_iter().for_each(|(k, v)| {
        let old = v1.nonces.insert(k, v);
        if let Some(old) = old {
            assert_eq!(old, v)
        }
    });
    v2.class_hashes.into_iter().for_each(|(k, v)| {
        let old = v1.class_hashes.insert(k, v);
        if let Some(old) = old {
            assert_eq!(old, v)
        }
    });
    v2.storage.into_iter().for_each(|(k, v)| {
        let old = v1.storage.insert(k, v);
        if let Some(old) = old {
            assert_eq!(old, v)
        }
    });

    v1
}

impl From<OfflineConsecutiveStateReaders> for StateCache {
    fn from(value: OfflineConsecutiveStateReaders) -> Self {
        let prev = value.offline_state_reader_prev_block;
        let contract_classes = prev.contract_class_mapping;
        let block_states = BlockState {
            nonces: prev.state_maps.nonces,
            class_hashes: prev.state_maps.class_hashes,
            storage: prev.state_maps.storage,
        };
        let block_number = value
            .block_context_next_block
            .block_info()
            .block_number
            .prev()
            .unwrap();
        Self {
            contract_classes,
            block_states: HashMap::from_iter(vec![(block_number, block_states)]),
            ..Default::default()
        }
    }
}
