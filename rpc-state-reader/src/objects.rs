//! This module contains custom objects
//! and how to deserialize them from RPC calls

use blockifier::{execution::stack_trace::ErrorStackSegment, transaction::objects::RevertError};
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use serde::{Deserialize, Serialize};
use starknet_api::{
    block::BlockStatus,
    hash::StarkHash,
    transaction::{
        fields::Fee, Event, MessageToL1, Transaction, TransactionExecutionStatus, TransactionHash,
    },
};
use starknet_gateway::rpc_objects::BlockHeader;

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
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
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
    #[serde(deserialize_with = "deser::deserialize_execution_resources")]
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
    #[serde(deserialize_with = "deser::deserialize_transaction")]
    pub transaction: Transaction,
}

/// Some types require their own deserializer, as their ir shape is slightly different
/// from the ones in starknet. This module contains such deserializaction functions.
pub mod deser {
    use std::collections::HashMap;

    use cairo_vm::{
        types::builtin_name::BuiltinName, vm::runners::cairo_runner::ExecutionResources,
    };
    use serde::{Deserialize, Deserializer};
    use starknet_api::transaction::{
        DeclareTransaction, DeployAccountTransaction, InvokeTransaction, Transaction,
    };

    pub fn deserialize_execution_resources<'de, D>(
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

    pub fn deserialize_transaction<'de, D>(deserializer: D) -> Result<Transaction, D::Error>
    where
        D: Deserializer<'de>,
    {
        let transaction: serde_json::Value = Deserialize::deserialize(deserializer)?;
        transaction_from_json(transaction).map_err(|err| serde::de::Error::custom(err.to_string()))
    }

    /// Freestanding deserialize method to avoid a new type.
    pub fn transaction_from_json(
        mut transaction: serde_json::Value,
    ) -> serde_json::Result<Transaction> {
        // uppercase fields to make it starknet compatible
        if let Some(resource_bounds) = transaction.get_mut("resource_bounds") {
            if let Some(l1_gas) = resource_bounds.get_mut("l1_gas") {
                resource_bounds["L1_GAS"] = l1_gas.clone();
                resource_bounds.as_object_mut().unwrap().remove("l1_gas");
            }
            if let Some(l2_gas) = resource_bounds.get_mut("l2_gas") {
                resource_bounds["L2_GAS"] = l2_gas.clone();
                resource_bounds.as_object_mut().unwrap().remove("l2_gas");
            }
        }

        #[derive(Deserialize)]
        struct Header {
            r#type: String,
            version: String,
        }
        let Header {
            r#type: tx_type,
            version: tx_version,
        } = serde_json::from_value(transaction.clone())?;

        match tx_type.as_str() {
            "INVOKE" => match tx_version.as_str() {
                "0x0" => Ok(Transaction::Invoke(InvokeTransaction::V0(
                    serde_json::from_value(transaction)?,
                ))),
                "0x1" => Ok(Transaction::Invoke(InvokeTransaction::V1(
                    serde_json::from_value(transaction)?,
                ))),
                "0x3" => Ok(Transaction::Invoke(InvokeTransaction::V3(
                    serde_json::from_value(transaction)?,
                ))),
                x => Err(serde::de::Error::custom(format!(
                    "unimplemented invoke version: {x}"
                ))),
            },
            "DEPLOY_ACCOUNT" => match tx_version.as_str() {
                "0x1" => Ok(Transaction::DeployAccount(DeployAccountTransaction::V1(
                    serde_json::from_value(transaction)?,
                ))),
                "0x3" => Ok(Transaction::DeployAccount(DeployAccountTransaction::V3(
                    serde_json::from_value(transaction)?,
                ))),
                x => Err(serde::de::Error::custom(format!(
                    "unimplemented declare version: {x}"
                ))),
            },
            "DECLARE" => match tx_version.as_str() {
                "0x0" => Ok(Transaction::Declare(DeclareTransaction::V0(
                    serde_json::from_value(transaction)?,
                ))),
                "0x1" => Ok(Transaction::Declare(DeclareTransaction::V1(
                    serde_json::from_value(transaction)?,
                ))),
                "0x2" => Ok(Transaction::Declare(DeclareTransaction::V2(
                    serde_json::from_value(transaction)?,
                ))),
                "0x3" => Ok(Transaction::Declare(DeclareTransaction::V3(
                    serde_json::from_value(transaction)?,
                ))),
                x => Err(serde::de::Error::custom(format!(
                    "unimplemented declare version: {x}"
                ))),
            },
            "L1_HANDLER" => Ok(Transaction::L1Handler(serde_json::from_value(transaction)?)),
            x => Err(serde::de::Error::custom(format!(
                "unimplemented transaction type deserialization: {x}"
            ))),
        }
    }
}
