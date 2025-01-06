//! This module contains custom objects
//! and how to deserialize them from RPC calls

use serde::{Deserialize, Serialize};
use starknet_api::{
    block::{BlockHash, BlockNumber, BlockStatus, BlockTimestamp, GasPrice},
    core::{ContractAddress, GlobalRoot},
    data_availability::L1DataAvailabilityMode,
    hash::StarkHash,
    transaction::{
        fields::Fee, Event, MessageToL1, Transaction, TransactionExecutionStatus, TransactionHash,
    },
};

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

#[derive(Clone, Debug, Serialize, Deserialize)]
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
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct FeePayment {
    pub amount: Fee,
    pub unit: String,
}

// The following structures are taken from https://github.com/starkware-libs/sequencer,
// but modified to suit our particular needs.

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct BlockHeader {
    pub block_hash: BlockHash,
    pub parent_hash: BlockHash,
    pub block_number: BlockNumber,
    pub sequencer_address: ContractAddress,
    pub new_root: GlobalRoot,
    pub timestamp: BlockTimestamp,
    pub l1_gas_price: ResourcePrice,
    pub l1_data_gas_price: ResourcePrice,
    pub l1_da_mode: L1DataAvailabilityMode,
    pub starknet_version: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct ResourcePrice {
    pub price_in_wei: GasPrice,
    pub price_in_fri: GasPrice,
}

/// Some types require their own deserializer, as their ir shape is slightly different
/// from the ones in starknet. This module contains such deserializaction functions.
pub mod deser {
    use serde::{Deserialize, Deserializer};
    use starknet_api::transaction::{
        DeclareTransaction, DeployAccountTransaction, InvokeTransaction, Transaction,
    };

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
