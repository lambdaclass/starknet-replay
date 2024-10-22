use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use serde::{Deserialize, Serialize};
use starknet::core::types::Transaction;
use starknet_api::{
    block::BlockStatus,
    hash::StarkHash,
    transaction::{Event, Fee, MessageToL1, TransactionExecutionStatus, TransactionHash},
};
use starknet_gateway::rpc_objects::BlockHeader;

// The following are not used right now
// We are keeping them just in case

#[derive(Debug, Deserialize, Clone, Eq, PartialEq)]
pub struct RpcTransactionTrace {
    pub validate_invocation: Option<RpcCallInfo>,
    #[serde(
        alias = "execute_invocation",
        alias = "constructor_invocation",
        alias = "function_invocation"
    )]
    pub execute_invocation: Option<RpcCallInfo>,
    pub fee_transfer_invocation: Option<RpcCallInfo>,
}

#[derive(Debug, Clone, Eq, PartialEq, Default, Deserialize)]
pub struct RpcCallInfo {
    pub result: Option<Vec<StarkHash>>,
    pub calldata: Option<Vec<StarkHash>>,
    pub calls: Vec<RpcCallInfo>,
    pub revert_reason: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RpcTransactionReceipt {
    pub transaction_hash: TransactionHash,
    pub block_hash: StarkHash,
    pub block_number: u64,
    #[serde(rename = "type")]
    pub tx_type: String,
    pub actual_fee: FeePayment,
    pub messages_sent: Vec<MessageToL1>,
    pub events: Vec<Event>,
    #[serde(flatten)]
    pub execution_status: TransactionExecutionStatus,
    #[serde(deserialize_with = "deser::vm_execution_resources_deser")]
    pub execution_resources: ExecutionResources,
}

#[derive(Debug, Deserialize)]
pub struct FeePayment {
    pub amount: Fee,
    pub unit: String,
}

// The following structures are taken from https://github.com/starkware-libs/sequencer,
// but modified to suit our particular needs.

#[derive(Debug, Deserialize, Serialize)]
pub struct BlockWithTxHahes {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BlockStatus>,
    #[serde(flatten)]
    pub header: BlockHeader,
    pub transactions: Vec<TransactionHash>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct BlockWithTxs {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<BlockStatus>,
    #[serde(flatten)]
    pub header: BlockHeader,
    pub transactions: Vec<TransactionWithHash>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TransactionWithHash {
    pub transaction_hash: TransactionHash,
    #[serde(flatten)]
    pub transaction: Transaction,
}

mod deser {
    use std::collections::HashMap;

    use cairo_vm::{
        types::builtin_name::BuiltinName, vm::runners::cairo_runner::ExecutionResources,
    };
    use serde::{Deserialize, Deserializer};

    pub fn vm_execution_resources_deser<'de, D>(
        deserializer: D,
    ) -> Result<ExecutionResources, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value: serde_json::Value = Deserialize::deserialize(deserializer)?;

        // Parse n_steps
        let n_steps: usize = serde_json::from_value(
            value
                .get("steps")
                .ok_or(serde::de::Error::custom("missing field `n_steps`"))?
                .clone(),
        )
        .map_err(|e| serde::de::Error::custom(e.to_string()))?;

        // Parse n_memory_holes
        let n_memory_holes: usize = if let Some(memory_holes) = value.get("memory_holes") {
            serde_json::from_value(memory_holes.clone())
                .map_err(|e| serde::de::Error::custom(e.to_string()))?
        } else {
            0
        };

        // Parse builtin instance counter
        let builtn_names: [BuiltinName; 8] = [
            BuiltinName::output,
            BuiltinName::range_check,
            BuiltinName::pedersen,
            BuiltinName::ecdsa,
            BuiltinName::keccak,
            BuiltinName::bitwise,
            BuiltinName::ec_op,
            BuiltinName::poseidon,
        ];
        let mut builtin_instance_counter = HashMap::new();
        for name in builtn_names {
            let builtin_counter: Option<usize> = value
                .get(format!("{}_applications", name.to_str()))
                .and_then(|a| serde_json::from_value(a.clone()).ok());
            if let Some(builtin_counter) = builtin_counter {
                if builtin_counter > 0 {
                    builtin_instance_counter.insert(name, builtin_counter);
                }
            };
        }

        Ok(ExecutionResources {
            n_steps,
            n_memory_holes,
            builtin_instance_counter,
        })
    }
}
