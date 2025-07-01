use serde::{Deserialize, Serialize};
use starknet_api::transaction::TransactionHash;
use starknet_core::types::{Event, ExecutionResources, ExecutionResult, FeePayment, MsgToL1};

#[derive(Serialize, Deserialize, Clone)]
pub struct RpcTransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub actual_fee: FeePayment,
    pub messages_sent: Vec<MsgToL1>,
    pub events: Vec<Event>,
    pub execution_resources: ExecutionResources,
    #[serde(flatten)]
    pub execution_result: ExecutionResult,
}
