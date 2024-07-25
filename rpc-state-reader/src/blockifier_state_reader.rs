use blockifier::{
    blockifier::block::BlockInfo,
    context::{BlockContext, ChainInfo, FeeTokenAddresses},
    execution::contract_class::{ClassInfo, ContractClass, ContractClassV0, ContractClassV0Inner},
    state::{
        cached_state::CachedState,
        errors::StateError,
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
use cairo_vm::types::program::Program;
use starknet::core::types::ContractClass as SNContractClass;
use starknet_api::{
    block::BlockNumber,
    core::{calculate_contract_address, ClassHash, CompiledClassHash, ContractAddress, Nonce},
    hash::StarkFelt,
    stark_felt,
    state::StorageKey,
    transaction::{Transaction as SNTransaction, TransactionHash},
};
use std::sync::Arc;

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

                if cfg!(feature = "only_casm") {
                    let casm_cc =
                    cairo_lang_starknet_classes::casm_contract_class::CasmContractClass::from_contract_class(sierra_cc, false, usize::MAX).unwrap();
                    ContractClass::V1(casm_cc.try_into().unwrap())
                } else {
                    ContractClass::V1Sierra(sierra_cc.try_into().unwrap())
                }
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

pub fn execute_tx(
    tx_hash: &str,
    network: RpcChain,
    block_number: BlockNumber,
) -> (
    TransactionExecutionInfo,
    TransactionTrace,
    RpcTransactionReceipt,
) {
    let tx_hash = tx_hash.strip_prefix("0x").unwrap();

    // Instantiate the RPC StateReader and the CachedState
    let rpc_reader = RpcStateReader(RpcState::new_rpc(network, block_number.into()).unwrap());
    let gas_price = rpc_reader.0.get_gas_price(block_number.0).unwrap();

    // Get values for block context before giving ownership of the reader
    let chain_id = rpc_reader.0.get_chain_name();
    let RpcBlockInfo {
        block_number,
        block_timestamp,
        sequencer_address,
        ..
    } = rpc_reader.0.get_block_info().unwrap();

    // Get transaction before giving ownership of the reader
    let tx_hash = TransactionHash(stark_felt!(tx_hash));
    let sn_api_tx = rpc_reader.0.get_transaction(&tx_hash);

    let trace = rpc_reader.0.get_transaction_trace(&tx_hash).unwrap();
    let receipt = rpc_reader.0.get_transaction_receipt(&tx_hash).unwrap();

    // Create state from RPC reader
    let mut state = CachedState::new(rpc_reader);

    // let fee_token_address =
    //     contract_address!("049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7");
    // const N_STEPS_FEE_WEIGHT: f64 = 0.01;
    // let vm_resource_fee_cost = Arc::new(HashMap::from([
    //     ("n_steps".to_string(), N_STEPS_FEE_WEIGHT),
    //     ("output_builtin".to_string(), 0.0),
    //     ("pedersen_builtin".to_string(), N_STEPS_FEE_WEIGHT * 32.0),
    //     ("range_check_builtin".to_string(), N_STEPS_FEE_WEIGHT * 16.0),
    //     ("ecdsa_builtin".to_string(), N_STEPS_FEE_WEIGHT * 2048.0),
    //     ("bitwise_builtin".to_string(), N_STEPS_FEE_WEIGHT * 64.0),
    //     ("ec_op_builtin".to_string(), N_STEPS_FEE_WEIGHT * 1024.0),
    //     ("poseidon_builtin".to_string(), N_STEPS_FEE_WEIGHT * 32.0),
    //     (
    //         "segment_arena_builtin".to_string(),
    //         N_STEPS_FEE_WEIGHT * 10.0,
    //     ),
    //     ("keccak_builtin".to_string(), N_STEPS_FEE_WEIGHT * 2048.0), // 2**11
    // ]));

    let block_info = BlockInfo {
        block_number,
        block_timestamp,
        sequencer_address,
        // TODO: Check gas_prices and use_kzg_da
        gas_prices: gas_price,
        use_kzg_da: false,
    };

    let chain_info = ChainInfo {
        chain_id,
        fee_token_addresses: FeeTokenAddresses::default(),
    };

    // TODO: Check BlockContext::new_unchecked
    let block_context = BlockContext::new_unchecked(
        &block_info,
        &chain_info,
        &VersionedConstants::latest_constants_with_overrides(u32::MAX, usize::MAX),
    );
    // let block_context = BlockContext {
    //     chain_id,
    //     block_number,
    //     block_timestamp,
    //     sequencer_address,
    //     // TODO: Add strk token address when updated
    //     fee_token_addresses: FeeTokenAddresses {
    //         strk_fee_token_address: fee_token_address,
    //         eth_fee_token_address: fee_token_address,
    //     },
    //     vm_resource_fee_cost,
    //     // TODO: Add strk l1 gas price when updated
    //     gas_prices: GasPrices {
    //         eth_l1_gas_price: gas_price.eth_l1_gas_price,
    //         strk_l1_gas_price: gas_price.strk_l1_gas_price,
    //     },
    //     invoke_tx_max_n_steps: 1_000_000,
    //     validate_max_n_steps: 1_000_000,
    //     max_recursion_depth: 500,
    // };

    // Map starknet_api transaction to blockifier's
    let blockifier_tx: AccountTransaction = match sn_api_tx.unwrap() {
        SNTransaction::Invoke(tx) => {
            let invoke = InvokeTransaction {
                tx,
                tx_hash,
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
                tx_hash,
                contract_address,
            })
        }
        SNTransaction::Declare(tx) => {
            // Fetch the contract_class from the next block (as we don't have it in the previous one)
            let next_block_state_reader = RpcStateReader(
                RpcState::new_rpc(network, (block_number.next()).unwrap().into()).unwrap(),
            );
            let contract_class = next_block_state_reader
                .get_compiled_contract_class(tx.class_hash())
                .unwrap();

            let class_info = calculate_class_info_for_testing(contract_class);

            let declare = DeclareTransaction::new(tx, tx_hash, class_info).unwrap();
            AccountTransaction::Declare(declare)
        }
        SNTransaction::L1Handler(tx) => {
            // As L1Hanlder is not an account transaction we execute it here and return the result
            let blockifier_tx = L1HandlerTransaction {
                tx,
                tx_hash,
                paid_fee_on_l1: starknet_api::transaction::Fee(u128::MAX),
            };
            return (
                blockifier_tx
                    .execute(&mut state, &block_context, true, true)
                    .unwrap(),
                trace,
                receipt,
            );
        }
        _ => unimplemented!(),
    };

    (
        // TODO Change charge_fee: true
        blockifier_tx
            .execute(&mut state, &block_context, false, true)
            .unwrap(),
        trace,
        receipt,
    )
}

fn calculate_class_info_for_testing(contract_class: ContractClass) -> ClassInfo {
    let sierra_program_length = match contract_class {
        ContractClass::V0(_) => 0,
        ContractClass::V1(_) => 100,
        ContractClass::V1Sierra(_) => 100,
    };
    ClassInfo::new(&contract_class, sierra_program_length, 100).unwrap()
}

pub fn execute_tx_configurable_with_state(
    tx_hash: &TransactionHash,
    tx: SNTransaction,
    block_info: BlockInfo,
    _skip_validate: bool,
    _skip_nonce_check: bool,
    state: &mut CachedState<RpcStateReader>,
) -> TransactionExecutionResult<TransactionExecutionInfo> {
    // let _fee_token_address = FeeTokenAddresses {
    //     strk_fee_token_address: ContractAddress::default(),
    //     eth_fee_token_address: ContractAddress(starknet_api::patricia_key!(
    //         "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"
    //     )),
    // };

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
    let blockifier_exec_info = execute_tx_configurable_with_state(
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

/// Executes a transaction with blockifier
///
/// Unlike `execute_tx_configurable`, it does not depend on our state reader
/// and can be used with any cached state. It already receives all context information
/// needed to execute the transaction.
pub fn execute_tx_with_blockifier(
    state: &mut CachedState<impl StateReader>,
    context: BlockContext,
    transaction: SNTransaction,
    transaction_hash: TransactionHash,
) -> TransactionExecutionResult<TransactionExecutionInfo> {
    let account_transaction: AccountTransaction = match transaction {
        SNTransaction::Invoke(tx) => {
            let invoke = InvokeTransaction {
                tx,
                tx_hash: transaction_hash,
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
                tx_hash: transaction_hash,
                contract_address,
            })
        }
        SNTransaction::Declare(tx) => {
            let contract_class = state
                .state
                .get_compiled_contract_class(tx.class_hash())
                .unwrap();

            let class_info = calculate_class_info_for_testing(contract_class);

            let declare = DeclareTransaction::new(tx, transaction_hash, class_info).unwrap();
            AccountTransaction::Declare(declare)
        }
        SNTransaction::L1Handler(tx) => {
            // As L1Hanlder is not an account transaction we execute it here and return the result
            let account_transaction = L1HandlerTransaction {
                tx,
                tx_hash: transaction_hash,
                paid_fee_on_l1: starknet_api::transaction::Fee(u128::MAX),
            };

            return account_transaction.execute(state, &context, true, true);
        }
        _ => unimplemented!(),
    };

    account_transaction.execute(state, &context, false, true)
}

pub fn fetch_block_context(state: &RpcState, block_number: BlockNumber) -> BlockContext {
    let rpc_block_info = state.get_block_info().unwrap();
    let gas_price = state.get_gas_price(block_number.0).unwrap();

    BlockContext::new_unchecked(
        &BlockInfo {
            block_number,
            block_timestamp: rpc_block_info.block_timestamp,
            sequencer_address: rpc_block_info.sequencer_address,
            gas_prices: gas_price,
            use_kzg_da: false,
        },
        &ChainInfo {
            chain_id: state.get_chain_name(),
            fee_token_addresses: Default::default(),
        },
        &VersionedConstants::latest_constants_with_overrides(u32::MAX, usize::MAX),
    )
}

#[cfg(test)]
mod tests {

    use std::num::NonZeroU128;

    use crate::rpc_state::{BlockValue, RpcCallInfo};

    use super::*;
    use blockifier::{
        execution::call_info::CallInfo,
        state::cached_state::StateChangesCount,
        transaction::objects::{GasVector, StarknetResources},
    };
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
        200304,
        RpcChain::MainNet
        => ignore["Doesn't revert in newest blockifier version"]
    )]
    #[test_case(
        "0x02b28b4846a756e0cec6385d6d13f811e745a88c7e75a3ebc5fead5b4af152a3",
        200303,
        RpcChain::MainNet
        => ignore["broken on both due to a cairo-vm error"]
    )]
    fn blockifier_test_case_reverted_tx(hash: &str, block_number: u64, chain: RpcChain) {
        // To reexecute a transaction, we must use the state from its previous block
        let previous_block = BlockNumber(block_number - 1);
        let (tx_info, trace, _) = execute_tx(hash, chain, previous_block);

        assert_eq!(
            tx_info.revert_error,
            trace.execute_invocation.unwrap().revert_reason
        );

        // We can't currently compare fee values
    }

    #[test_case(
        // Declare tx
        "0x60506c49e65d84e2cdd0e9142dc43832a0a59cb6a9cbcce1ab4f57c20ba4afb",
        347900,
        RpcChain::MainNet
        => ignore
    )]
    #[test_case(
        // Declare tx
        "0x1088aa18785779e1e8eef406dc495654ad42a9729b57969ad0dbf2189c40bee",
        271888,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x014640564509873cf9d24a311e1207040c8b60efd38d96caef79855f0b0075d5",
        90007,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x025844447697eb7d5df4d8268b23aef6c11de4087936048278c2559fc35549eb",
        197001,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x00164bfc80755f62de97ae7c98c9d67c1767259427bcf4ccfcc9683d44d54676",
        197001,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x05d200ef175ba15d676a68b36f7a7b72c17c17604eda4c1efc2ed5e4973e2c91",
        169929,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x0528ec457cf8757f3eefdf3f0728ed09feeecc50fd97b1e4c5da94e27e9aa1d6",
        169929,
        RpcChain::MainNet
        => ignore
    )]
    #[test_case(
        "0x0737677385a30ec4cbf9f6d23e74479926975b74db3d55dc5e46f4f8efee41cf",
        169929,
        RpcChain::MainNet
        => ignore
    )]
    #[test_case(
        "0x026c17728b9cd08a061b1f17f08034eb70df58c1a96421e73ee6738ad258a94c",
        169929,
        RpcChain::MainNet
    )]
    #[test_case(
        // review later
        "0x0743092843086fa6d7f4a296a226ee23766b8acf16728aef7195ce5414dc4d84",
        186549,
        RpcChain::MainNet
    )]
    #[test_case(
        // fails in blockifier
        "0x00724fc4a84f489ed032ebccebfc9541eb8dc64b0e76b933ed6fc30cd6000bd1",
        186552,
        RpcChain::MainNet
        => ignore
    )]
    #[test_case(
        "0x176a92e8df0128d47f24eebc17174363457a956fa233cc6a7f8561bfbd5023a",
        317093,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x04db9b88e07340d18d53b8b876f28f449f77526224afb372daaf1023c8b08036",
        398052,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x5a5de1f42f6005f3511ea6099daed9bcbcf9de334ee714e8563977e25f71601",
        281514,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x26be3e906db66973de1ca5eec1ddb4f30e3087dbdce9560778937071c3d3a83",
        351269,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x4f552c9430bd21ad300db56c8f4cae45d554a18fac20bf1703f180fac587d7e",
        351226,
        RpcChain::MainNet
    )]
    // DeployAccount for different account providers:

    // OpenZeppelin (v0.7.0)
    #[test_case(
        "0x04df8a364233d995c33c7f4666a776bf458631bec2633e932b433a783db410f8",
        422882,
        RpcChain::MainNet
    )]
    // Argent X (v5.7.0)
    #[test_case(
        "0x74820d4a1ac6e832a51a8938959e6f15a247f7d34daea2860d4880c27bc2dfd",
        475946,
        RpcChain::MainNet
        => ignore
    )]
    #[test_case(
        "0x41497e62fb6798ff66e4ad736121c0164cdb74005aa5dab025be3d90ad4ba06",
        638867,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x7805c2bf5abaf4fe0eb1db7b7be0486a14757b4bf96634c828d11c07e4a763c",
        641976,
        RpcChain::MainNet
        => ignore
    )]
    #[test_case(
        "0x73ef9cde09f005ff6f411de510ecad4cdcf6c4d0dfc59137cff34a4fc74dfd",
        654001,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x75d7ef42a815e4d9442efcb509baa2035c78ea6a6272ae29e87885788d4c85e",
        654001,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x1ecb4b825f629eeb9816ddfd6905a85f6d2c89995907eacaf6dc64e27a2c917",
        654001,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x70d83cb9e25f1e9f7be2608f72c7000796e4a222c1ed79a0ea81abe5172557b",
        654001,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x670321c71835004fcab639e871ef402bb807351d126ccc4d93075ff2c31519d",
        654001,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x5896b4db732cfc57ce5d56ece4dfa4a514bd435a0ee80dc79b37e60cdae5dd6",
        653001,
        RpcChain::MainNet
        => ignore["takes to long"]
    )]
    #[test_case(
        "0x5a030fd81f14a1cf29a2e5259d3f2c9960018ade2d135269760e6fb4802ac02",
        653001,
        RpcChain::MainNet
        => ignore["halts execution"]
    )]
    #[test_case(
        "0x2d2bed435d0b43a820443aad2bc9e3d4fa110c428e65e422101dfa100ba5664",
        653001,
        RpcChain::MainNet
        => ignore
    )]
    #[test_case(
        "0x3330b29e8b99dedef79f5c7cdc2b510c590155add29dcc5e2f92d176d8e19d",
        653001,
        RpcChain::MainNet
        => ignore
    )]
    fn blockifier_tx(hash: &str, block_number: u64, chain: RpcChain) {
        // To reexecute a transaction, we must use the state from its previous block
        let previous_block = BlockNumber(block_number - 1);
        let (tx_info, trace, _receipt) = execute_tx(hash, chain, previous_block);

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

    // test cairo-vm's tx execution against cairo-native, using only_cairo_vm feature
    #[test_case(
        "0x04ba569a40a866fd1cbb2f3d3ba37ef68fb91267a4931a377d6acc6e5a854f9a",
        648462,
        RpcChain::MainNet,
        GasVector { l1_gas: 4646, l1_data_gas: 0 },
        7,
        3,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 3,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false
    )]
    #[test_case(
        "0x0355059efee7a38ba1fd5aef13d261914608dce7bdfacad92a71e396f0ad7a77",
        661815,
        RpcChain::MainNet,
        GasVector { l1_gas: 4646, l1_data_gas: 0 },
        9,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 3,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false
    )]
    #[test_case(
        "0x05324bac55fb9fb53e738195c2dcc1e7fed1334b6db824665e3e984293bec95e",
        662246,
        RpcChain::MainNet,
        GasVector { l1_gas: 4646, l1_data_gas: 0 },
        9,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 3,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false
    )]
    #[test_case(
        "0x670321c71835004fcab639e871ef402bb807351d126ccc4d93075ff2c31519d",
        654001,
        RpcChain::MainNet,
        GasVector { l1_gas: 4646, l1_data_gas: 0 },
        7,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 3,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false
    )]
    #[test_case(
        "0x06962f11a96849ebf05cd222313858a93a8c5f300493ed6c5859dd44f5f2b4e3",  
        654770,
        RpcChain::MainNet,
        GasVector { l1_gas: 4646, l1_data_gas: 0 },
        7,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 3,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false
    )]
    #[test_case(
        "0x078b81326882ecd2dc6c5f844527c3f33e0cdb52701ded7b1aa4d220c5264f72",
        653019,
        RpcChain::MainNet,
        GasVector { l1_gas: 11736, l1_data_gas: 0 },
        28,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 8,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 4,
        },
        false
    )]
    #[test_case(
        "0x0780e3a498b4fd91ab458673891d3e8ee1453f9161f4bfcb93dd1e2c91c52e10",
        650558,
        RpcChain::MainNet,
        GasVector { l1_gas: 6538, l1_data_gas: 0 },
        24,
        3,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 4,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 3,
        },
        false
    )]
    #[test_case(
        "0x4f552c9430bd21ad300db56c8f4cae45d554a18fac20bf1703f180fac587d7e",
        351226,
        RpcChain::MainNet,
        GasVector { l1_gas: 2754, l1_data_gas: 0 },
        3,
        0,
        0,
        Some(3),
        StateChangesCount {
            n_storage_updates: 2,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        false
    )]
    #[test_case(
        "0x176a92e8df0128d47f24eebc17174363457a956fa233cc6a7f8561bfbd5023a",
        317093,
        RpcChain::MainNet,
        GasVector { l1_gas: 1652, l1_data_gas: 0 },
        6,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 1,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        false
    )]
    #[test_case(
        "0x026c17728b9cd08a061b1f17f08034eb70df58c1a96421e73ee6738ad258a94c",
        169929,
        RpcChain::MainNet,
        GasVector { l1_gas: 1652, l1_data_gas: 0 },
        8,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 1,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        false
    )]
    #[test_case(
        "0x1088aa18785779e1e8eef406dc495654ad42a9729b57969ad0dbf2189c40bee",
        271888,
        RpcChain::MainNet,
        GasVector { l1_gas: 1652, l1_data_gas: 0 },
        0,
        2,
        42564,
        None,
        StateChangesCount {
            n_storage_updates: 1,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        false
    )]
    #[test_case(
        "0x73ef9cde09f005ff6f411de510ecad4cdcf6c4d0dfc59137cff34a4fc74dfd",
        654001,
        RpcChain::MainNet,
        GasVector { l1_gas: 2754, l1_data_gas: 0 },
        5,
        0,
        0,
        Some(5),
        StateChangesCount {
            n_storage_updates: 2,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        false
    )]
    #[test_case(
        "0x0743092843086fa6d7f4a296a226ee23766b8acf16728aef7195ce5414dc4d84",
        186549,
        RpcChain::MainNet,
        GasVector { l1_gas: 5748, l1_data_gas: 0 },
        7,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 4,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false
    )]
    #[test_case(
        "0x066e1f01420d8e433f6ef64309adb1a830e5af0ea67e3d935de273ca57b3ae5e",
        662252,
        RpcChain::MainNet,
        GasVector { l1_gas: 6850, l1_data_gas: 0 },
        18,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 5,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false
    )]
    #[test_case(
        "0x04756d898323a8f884f5a6aabd6834677f4bbaeecc2522f18b3ae45b3f99cd1e",
        662250,
        RpcChain::MainNet,
        GasVector { l1_gas: 1652, l1_data_gas: 0 },
        10,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 1,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        false
    )]
    #[test_case(
        "0x00f390691fd9e865f5aef9c7cc99889fb6c2038bc9b7e270e8a4fe224ccd404d",
        662251,
        RpcChain::MainNet,
        GasVector { l1_gas: 3544, l1_data_gas: 0 },
        12,
        5,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 2,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false
    )]
    #[test_case(
        "0x26be3e906db66973de1ca5eec1ddb4f30e3087dbdce9560778937071c3d3a83",
        351269,
        RpcChain::MainNet,
        GasVector { l1_gas: 2754, l1_data_gas: 0 },
        3,
        0,
        0,
        Some(3),
        StateChangesCount {
            n_storage_updates: 2,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        false
    )]
    #[test_case(
        "0x0310c46edc795c82c71f600159fa9e6c6540cb294df9d156f685bfe62b31a5f4",
        662249,
        RpcChain::MainNet,
        GasVector { l1_gas: 9844, l1_data_gas: 0 },
        37,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 7,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 3,
        },
        false
    )]
    #[test_case(
        "0x06a09ffbf996178ac6e90101047e42fe29cb7108573b2ecf4b0ebd2cba544cb4",
        662248,
        RpcChain::MainNet,
        GasVector { l1_gas: 5748, l1_data_gas: 0 },
        4,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 4,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false
    )]
    #[test_case(
        "0x026e04e96ba1b75bfd066c8e138e17717ecb654909e6ac24007b644ac23e4b47",
        536893,
        RpcChain::MainNet,
        GasVector { l1_gas: 13940, l1_data_gas: 0 },
        24,
        4,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 10,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 4,
        },
        false
    )]
    #[test_case(
        "0x01351387ef63fd6fe5ec10fa57df9e006b2450b8c68d7eec8cfc7d220abc7eda",
        644700,
        RpcChain::MainNet,
        GasVector { l1_gas: 1652, l1_data_gas: 0 },
        8,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 1,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        true
    )]
    #[allow(clippy::too_many_arguments)]
    fn test_transaction_info(
        hash: &str,
        block_number: u64,
        chain: RpcChain,
        da_gas: GasVector,
        calldata_length: usize,
        signature_length: usize,
        code_size: usize,
        l1_handler_payload_size: Option<usize>,
        starknet_chg: StateChangesCount,
        is_reverted: bool,
    ) {
        let previous_block = BlockNumber(block_number - 1);
        let (tx_info, _, _) = execute_tx(hash, chain, previous_block);
        let starknet_resources = tx_info.actual_resources.starknet_resources;
        let callinfo_iter = match tx_info.execute_call_info {
            Some(c) => vec![c],
            None => vec![CallInfo::default()], // there's no call info, so we take the default value to have all of it's atributes set to 0
        };
        let starknet_rsc = StarknetResources::new(
            calldata_length,
            signature_length,
            code_size,
            starknet_chg,
            l1_handler_payload_size,
            callinfo_iter.iter(),
        );

        assert_eq!(is_reverted, tx_info.revert_error.is_some());
        assert_eq!(da_gas, tx_info.da_gas);
        assert_eq!(starknet_rsc, starknet_resources);
        assert_eq!(
            starknet_rsc.state_changes_for_fee,
            starknet_resources.state_changes_for_fee
        );
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
