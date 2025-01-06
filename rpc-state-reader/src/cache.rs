use std::collections::HashMap;

use cairo_vm::Felt252;
use serde::{Deserialize, Serialize};
use serde_with::serde_as;
use starknet::core::types::ContractClass;
use starknet_api::{
    core::{ClassHash, ContractAddress, Nonce},
    state::StorageKey,
    transaction::{Transaction, TransactionHash},
};

use crate::objects::{BlockWithTxHahes, RpcTransactionReceipt};

#[serde_as]
#[derive(Default, Serialize, Deserialize)]
pub struct RpcCachedState {
    pub get_block_with_tx_hashes: Option<BlockWithTxHahes>,
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
}
