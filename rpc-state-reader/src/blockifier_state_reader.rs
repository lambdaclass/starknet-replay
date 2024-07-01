use blockifier::{
    blockifier::block::BlockInfo,
    context::{BlockContext, ChainInfo, FeeTokenAddresses},
    execution::contract_class::{
        ClassInfo, ContractClass, ContractClassV0, ContractClassV0Inner, SierraContractClassV1,
        SierraContractClassV1Inner,
    },
    state::{
        cached_state::CachedState,
        errors::StateError,
        global_cache::GlobalContractCache,
        state_api::{StateReader, StateResult},
    },
    transaction::{
        account_transaction::AccountTransaction,
        objects::{TransactionExecutionInfo, TransactionExecutionResult},
        transactions::{
            DeclareTransaction, DeployAccountTransaction, ExecutableTransaction, InvokeTransaction,
            L1HandlerTransaction,
        },
    },
    versioned_constants::VersionedConstants,
};
use cairo_lang_starknet_classes::casm_contract_class::CasmContractClass;
use cairo_vm::types::program::Program;
use starknet::core::types::ContractClass as SNContractClass;
use starknet_api::{
    block::BlockNumber,
    contract_address,
    core::{
        calculate_contract_address, ClassHash, CompiledClassHash, ContractAddress, Nonce,
        PatriciaKey,
    },
    hash::{StarkFelt, StarkHash},
    patricia_key, stark_felt,
    state::StorageKey,
    transaction::{Transaction as SNTransaction, TransactionHash},
};
use std::{collections::HashMap, sync::Arc};

use crate::{
    rpc_state::{RpcBlockInfo, RpcChain, RpcState, RpcTransactionReceipt, TransactionTrace},
    utils,
};

pub struct RpcStateReader(pub RpcState);

impl RpcStateReader {
    pub fn new(state: RpcState) -> Self {
        Self(state)
    }
}

impl StateReader for RpcStateReader {
    fn get_storage_at(
        &self,
        contract_address: starknet_api::core::ContractAddress,
        key: StorageKey,
    ) -> StateResult<StarkFelt> {
        Ok(self.0.get_storage_at(&contract_address, &key))
    }

    fn get_nonce_at(&self, contract_address: ContractAddress) -> StateResult<Nonce> {
        Ok(Nonce(self.0.get_nonce_at(&contract_address)))
    }

    fn get_class_hash_at(&self, contract_address: ContractAddress) -> StateResult<ClassHash> {
        Ok(self.0.get_class_hash_at(&contract_address))
    }

    /// Returns the contract class of the given class hash.
    fn get_compiled_contract_class(
        &self,
        class_hash: starknet_api::core::ClassHash,
    ) -> StateResult<ContractClass> {
        Ok(match self.0.get_contract_class(&class_hash) {
            Some(SNContractClass::Legacy(compressed_legacy_cc)) => {
                let as_str = utils::decode_reader(compressed_legacy_cc.program).unwrap();
                let program = Program::from_bytes(as_str.as_bytes(), None).unwrap();
                let entry_points_by_type = utils::map_entry_points_by_type_legacy(
                    compressed_legacy_cc.entry_points_by_type,
                );
                let inner = Arc::new(ContractClassV0Inner {
                    program,
                    entry_points_by_type,
                });
                ContractClass::V0(ContractClassV0(inner))
            }
            Some(SNContractClass::Sierra(flattened_sierra_cc)) => {
                let middle_sierra: utils::MiddleSierraContractClass = {
                    let v = serde_json::to_value(flattened_sierra_cc).unwrap();
                    serde_json::from_value(v).unwrap()
                };
                let sierra_cc = cairo_lang_starknet_classes::contract_class::ContractClass {
                    sierra_program: middle_sierra.sierra_program,
                    contract_class_version: middle_sierra.contract_class_version,
                    entry_points_by_type: middle_sierra.entry_points_by_type,
                    sierra_program_debug_info: None,
                    abi: None,
                };
                ContractClass::V1Sierra(sierra_cc.try_into().unwrap())
            }
            None => {
                return Err(StateError::UndeclaredClassHash(
                    starknet_api::core::ClassHash(*class_hash),
                ))
            }
        })
    }

    /// Returns the compiled class hash of the given class hash.
    fn get_compiled_class_hash(&self, class_hash: ClassHash) -> StateResult<CompiledClassHash> {
        Ok(CompiledClassHash(
            self.0
                .get_class_hash_at(&ContractAddress(class_hash.0.try_into().unwrap()))
                .0,
        ))
    }
}

fn calculate_class_info_for_testing(contract_class: ContractClass) -> ClassInfo {
    let sierra_program_length = match contract_class {
        ContractClass::V0(_) => 0,
        ContractClass::V1(_) => 100,
        ContractClass::V1Sierra(_) => 100,
    };
    ClassInfo::new(&contract_class, sierra_program_length, 100).unwrap()
}

/// Returns the execution information of a given transaction
pub fn get_tx_execution_info(
    tx_hash: &TransactionHash,
    tx: SNTransaction,
    block_info: BlockInfo,
    skip_validate: bool,
    skip_nonce_check: bool,
    state: &mut CachedState<RpcStateReader>,
) -> TransactionExecutionResult<TransactionExecutionInfo> {
    let fee_token_address = FeeTokenAddresses {
        strk_fee_token_address: ContractAddress::default(),
        eth_fee_token_address: ContractAddress(starknet_api::patricia_key!(
            "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"
        )),
    };

    // Get values for block context before giving ownership of the reader
    let chain_id = state.state.0.get_chain_name();

    let chain_info = ChainInfo {
        chain_id,
        fee_token_addresses: FeeTokenAddresses::default(),
    };

    let block_context = BlockContext::new_unchecked(
        &block_info,
        &chain_info,
        &VersionedConstants::latest_constants_with_overrides(u32::MAX, usize::MAX),
    );

    // Get transaction before giving ownership of the reader
    let blockifier_tx: AccountTransaction = match tx {
        SNTransaction::Invoke(tx) => {
            let invoke = InvokeTransaction {
                tx,
                tx_hash: *tx_hash,
                only_query: false,
            };
            AccountTransaction::Invoke(invoke)
        }
        SNTransaction::DeployAccount(tx) => {
            let contract_address = calculate_contract_address(
                tx.contract_address_salt(),
                tx.class_hash(),
                &tx.constructor_calldata(),
                ContractAddress::default(),
            )
            .unwrap();
            AccountTransaction::DeployAccount(DeployAccountTransaction {
                only_query: false,
                tx,
                tx_hash: *tx_hash,
                contract_address,
            })
        }
        SNTransaction::Declare(tx) => {
            let contract_class = state
                .state
                .get_compiled_contract_class(tx.class_hash())
                .unwrap();

            let class_info = calculate_class_info_for_testing(contract_class);

            let declare = DeclareTransaction::new(tx, *tx_hash, class_info).unwrap();
            AccountTransaction::Declare(declare)
        }
        SNTransaction::L1Handler(tx) => {
            // As L1Hanlder is not an account transaction we execute it here and return the result
            let blockifier_tx = L1HandlerTransaction {
                tx,
                tx_hash: *tx_hash,
                paid_fee_on_l1: starknet_api::transaction::Fee(u128::MAX),
            };
            return blockifier_tx.execute(state, &block_context, true, true);
        }
        _ => unimplemented!(),
    };

    #[cfg(not(feature = "cairo-native"))]
    let blockifier_execution = blockifier_tx.execute(state, &block_context, false, true);

    blockifier_execution
}

/// executes a transaction and returns its trace, receipt and execution information
pub fn execute_tx_configurable(
    state: &mut CachedState<RpcStateReader>,
    tx_hash: &str,
    block_number: BlockNumber,
    skip_validate: bool,
    skip_nonce_check: bool,
) -> TransactionExecutionResult<(
    TransactionExecutionInfo,
    TransactionTrace,
    RpcTransactionReceipt,
)> {
    let tx_hash =
        TransactionHash(StarkFelt::try_from(tx_hash.strip_prefix("0x").unwrap()).unwrap());
    let tx = state.state.0.get_transaction(&tx_hash).unwrap();
    let gas_price = state.state.0.get_gas_price(block_number.0).unwrap();
    let RpcBlockInfo {
        block_timestamp,
        sequencer_address,
        ..
    } = state.state.0.get_block_info().unwrap();

    let block_info = BlockInfo {
        block_number,
        block_timestamp,
        sequencer_address,
        // TODO: Check gas_prices and use_kzg_da
        gas_prices: gas_price,
        use_kzg_da: false,
    };
    let blockifier_exec_info = get_tx_execution_info(
        &tx_hash,
        tx,
        block_info,
        skip_validate,
        skip_nonce_check,
        state,
    )?;
    let trace = state.state.0.get_transaction_trace(&tx_hash).unwrap();
    let receipt = state.state.0.get_transaction_receipt(&tx_hash).unwrap();
    Ok((blockifier_exec_info, trace, receipt))
}

pub fn build_cached_state(network: &str, current_block_number: u64) -> CachedState<RpcStateReader> {
    let previous_block_number = BlockNumber(current_block_number - 1);
    let rpc_chain = parse_network(&network);
    let rpc_reader = RpcStateReader(
        RpcState::new_rpc(rpc_chain, previous_block_number.into())
            .expect("failed to create state reader"),
    );

    CachedState::new(rpc_reader)
}

pub fn parse_network(network: &str) -> RpcChain {
    match network.to_lowercase().as_str() {
        "mainnet" => RpcChain::MainNet,
        "testnet" => RpcChain::TestNet,
        "testnet2" => RpcChain::TestNet2,
        _ => panic!("Invalid network name, it should be one of: mainnet, testnet, testnet2"),
    }
}

#[cfg(test)]
mod tests {

    use std::num::NonZeroU128;

    use crate::rpc_state::{BlockValue, RpcCallInfo};

    use super::*;
    use blockifier::execution::call_info::CallInfo;
    use pretty_assertions_sorted::assert_eq_sorted;
    use test_case::test_case;
    #[test]
    fn test_get_gas_price() {
        let block = BlockValue::Number(BlockNumber(169928));
        let rpc_state = RpcState::new_rpc(RpcChain::MainNet, block).unwrap();

        let price = rpc_state.get_gas_price(169928).unwrap();
        assert_eq!(
            price.eth_l1_gas_price,
            NonZeroU128::new(22804578690).unwrap()
        );
    }

    #[test_case(
    "0x00b6d59c19d5178886b4c939656167db0660fe325345138025a3cc4175b21897",
    200303, // real block     200304
    RpcChain::MainNet => ignore["Doesn't revert in newest blockifier version"]
    )]
    #[test_case(
    "0x02b28b4846a756e0cec6385d6d13f811e745a88c7e75a3ebc5fead5b4af152a3",
    200302, // real block     200304
    RpcChain::MainNet
    => ignore["broken on both due to a cairo-vm error"]
)]
    fn blockifier_test_case_reverted_tx(hash: &str, block_number: u64, chain: RpcChain) {
        let mut state = build_cached_state(&chain.to_string(), block_number);
        let (tx_info, trace, _) = execute_tx_configurable(&mut state, hash, BlockNumber(block_number), false, false).unwrap();

        assert_eq!(
            tx_info.revert_error,
            trace.execute_invocation.unwrap().revert_reason
        );

        // We can't currently compare fee values
    }

    #[test_case(
        "0x05d200ef175ba15d676a68b36f7a7b72c17c17604eda4c1efc2ed5e4973e2c91",
        169928,
        RpcChain::MainNet => ignore["Current blockifier version is not currently in production, no recent tx available for testing"]
        )]
    #[test_case(
    // Declare tx
    "0x60506c49e65d84e2cdd0e9142dc43832a0a59cb6a9cbcce1ab4f57c20ba4afb",
    347899, // real block 347900
    RpcChain::MainNet
)]
    #[test_case(
    // Declare tx
    "0x1088aa18785779e1e8eef406dc495654ad42a9729b57969ad0dbf2189c40bee",
    271887, // real block 271888
    RpcChain::MainNet
)]
    #[test_case(
        "0x014640564509873cf9d24a311e1207040c8b60efd38d96caef79855f0b0075d5",
        90006,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x025844447697eb7d5df4d8268b23aef6c11de4087936048278c2559fc35549eb",
        197000,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x00164bfc80755f62de97ae7c98c9d67c1767259427bcf4ccfcc9683d44d54676",
        197000,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x05d200ef175ba15d676a68b36f7a7b72c17c17604eda4c1efc2ed5e4973e2c91",
        169928, // real block 169929
        RpcChain::MainNet
    )]
    #[test_case(
        "0x0528ec457cf8757f3eefdf3f0728ed09feeecc50fd97b1e4c5da94e27e9aa1d6",
        169928, // real block 169929
        RpcChain::MainNet
    )]
    #[test_case(
        "0x0737677385a30ec4cbf9f6d23e74479926975b74db3d55dc5e46f4f8efee41cf",
        169928, // real block 169929
        RpcChain::MainNet
    )]
    #[test_case(
        "0x026c17728b9cd08a061b1f17f08034eb70df58c1a96421e73ee6738ad258a94c",
        169928, // real block 169929
        RpcChain::MainNet
    )]
    #[test_case(
        // review later
        "0x0743092843086fa6d7f4a296a226ee23766b8acf16728aef7195ce5414dc4d84",
        186548, // real block     186549
        RpcChain::MainNet
    )]
    #[test_case(
        // fails in blockifier
        "0x00724fc4a84f489ed032ebccebfc9541eb8dc64b0e76b933ed6fc30cd6000bd1",
        186551, // real block     186552
        RpcChain::MainNet
    )]
    #[test_case(
    "0x176a92e8df0128d47f24eebc17174363457a956fa233cc6a7f8561bfbd5023a",
    317092, // real block 317093
    RpcChain::MainNet
)]
    #[test_case(
    "0x04db9b88e07340d18d53b8b876f28f449f77526224afb372daaf1023c8b08036",
    398051, // real block 398052
    RpcChain::MainNet
)]
    #[test_case(
    "0x5a5de1f42f6005f3511ea6099daed9bcbcf9de334ee714e8563977e25f71601",
    281513, // real block 281514
    RpcChain::MainNet
)]
    #[test_case(
    "0x26be3e906db66973de1ca5eec1ddb4f30e3087dbdce9560778937071c3d3a83",
    351268, // real block 351269
    RpcChain::MainNet
)]
    #[test_case(
    "0x4f552c9430bd21ad300db56c8f4cae45d554a18fac20bf1703f180fac587d7e",
    351225, // real block 351226
    RpcChain::MainNet
)]
    // DeployAccount for different account providers:

    // OpenZeppelin (v0.7.0)
    #[test_case(
    "0x04df8a364233d995c33c7f4666a776bf458631bec2633e932b433a783db410f8",
    422881, // real block 422882
    RpcChain::MainNet
)]
    // Argent X (v5.7.0)
    #[test_case(
            "0x74820d4a1ac6e832a51a8938959e6f15a247f7d34daea2860d4880c27bc2dfd",
            475945, // real block 475946
            RpcChain::MainNet
        )]
    #[test_case(
    "0x41497e62fb6798ff66e4ad736121c0164cdb74005aa5dab025be3d90ad4ba06",
    638866, // real block 475946
    RpcChain::MainNet
)]
    #[test_case(
    "0x7805c2bf5abaf4fe0eb1db7b7be0486a14757b4bf96634c828d11c07e4a763c",
    641975, // real block 475946
    RpcChain::MainNet
)]
    fn blockifier_tx(hash: &str, block_number: u64, chain: RpcChain) {
        // Execute using blockifier
        let mut state = build_cached_state(&chain.to_string(), block_number);
        let (tx_info, trace, _) = execute_tx_configurable(&mut state, hash, BlockNumber(block_number), false, false).unwrap();

        // We cannot currently check fee & resources

        // Compare tx CallInfos against trace RpcCallInfos
        // Note: This will check calldata, retdata, internal calls and make sure the tx is not reverted.
        // It will not chekced accessed or modified storage, messanges, and events (as they are not currenlty part of the RpcCallInfo)
        assert_eq_sorted!(
            tx_info.validate_call_info.map(|ref ci| ci.into()),
            trace.validate_invocation
        );
        assert_eq_sorted!(
            tx_info.execute_call_info.map(|ref ci| ci.into()),
            trace.execute_invocation
        );
        //assert_eq!(tx_info.fee_transfer_call_info.map(|ref ci| ci.into()), trace.fee_transfer_invocation); TODO: fix charge_fee
    }

    // Impl conversion for easier checking against RPC data
    impl From<&CallInfo> for RpcCallInfo {
        fn from(value: &CallInfo) -> Self {
            Self {
                retdata: Some(value.execution.retdata.0.clone()),
                calldata: Some((*value.call.calldata.0).clone()),
                internal_calls: value.inner_calls.iter().map(|ci| ci.into()).collect(),
                // We don't have the revert reason string in the trace so we just make sure it doesn't revert
                revert_reason: value.execution.failed.then_some("Default String".into()),
            }
        }
    }
}
