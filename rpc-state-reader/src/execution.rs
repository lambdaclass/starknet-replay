use crate::{
    objects::BlockHeader,
    reader::{RpcChain, RpcStateReader, StateReader},
};
use anyhow::Context;
use blockifier::{
    blockifier::block::validated_gas_prices,
    bouncer::BouncerConfig,
    context::{BlockContext, ChainInfo, FeeTokenAddresses},
    state::cached_state::CachedState,
    transaction::{
        account_transaction::ExecutionFlags, objects::TransactionExecutionInfo,
        transaction_execution::Transaction as BlockiTransaction,
        transactions::ExecutableTransaction,
    },
    versioned_constants::{VersionedConstants, VersionedConstantsOverrides},
};
use blockifier_reexecution::state_reader::compile::{
    legacy_to_contract_class_v0, sierra_to_versioned_contract_class_v1,
};
use starknet::core::types::ContractClass;
use starknet_api::{
    block::{BlockInfo, BlockNumber, GasPrice, NonzeroGasPrice},
    contract_class::{ClassInfo, SierraVersion},
    core::ContractAddress,
    patricia_key,
    test_utils::MAX_FEE,
    transaction::{Transaction as SNTransaction, TransactionHash},
};

pub fn fetch_block_context(reader: &impl StateReader) -> anyhow::Result<BlockContext> {
    let block = reader.get_block_with_tx_hashes()?;
    let block_info = get_block_info(block.header);

    let mut versioned_constants =
        VersionedConstants::get_versioned_constants(VersionedConstantsOverrides {
            validate_max_n_steps: u32::MAX,
            invoke_tx_max_n_steps: u32::MAX,
            max_recursion_depth: usize::MAX,
        });
    versioned_constants.disable_cairo0_redeclaration = false;

    let fee_token_addresses = FeeTokenAddresses {
        strk_fee_token_address: ContractAddress(patricia_key!(
            "0x04718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
        )),
        eth_fee_token_address: ContractAddress(patricia_key!(
            "0x049d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"
        )),
    };
    let chain_info = ChainInfo {
        chain_id: reader.get_chain_id(),
        fee_token_addresses,
    };

    Ok(BlockContext::new(
        block_info,
        chain_info,
        versioned_constants,
        BouncerConfig::max(),
    ))
}

pub fn fetch_blockifier_transaction(
    reader: &impl StateReader,
    flags: ExecutionFlags,
    hash: TransactionHash,
) -> anyhow::Result<BlockiTransaction> {
    let transaction = reader.get_transaction(&hash)?;

    let class_info = if let SNTransaction::Declare(declare) = &transaction {
        let class = reader.get_contract_class(&declare.class_hash())?;
        Some(get_class_info(class)?)
    } else {
        None
    };

    let fee = if let SNTransaction::L1Handler(_) = &transaction {
        Some(MAX_FEE)
    } else {
        None
    };

    let transaction = BlockiTransaction::from_api(transaction, hash, class_info, fee, None, flags)?;

    Ok(transaction)
}

/// Fetches and executes the given transaction.
///
/// Internally, it creates its own blank state, so it may fail when executing
/// a transaction in the middle of a block, if it depends on a previous transaction
/// of the same block.
///
/// It doesn't use the rpc cache.
pub fn execute_transaction(
    hash: &TransactionHash,
    block_number: BlockNumber,
    chain: RpcChain,
    flags: ExecutionFlags,
) -> anyhow::Result<TransactionExecutionInfo> {
    let (transaction, context) = fetch_transaction(hash, block_number, chain, flags)?;

    let previous_block_number = block_number
        .prev()
        .context("block number had no previous")?;
    let previous_reader = RpcStateReader::new(chain, previous_block_number);
    let mut state = CachedState::new(previous_reader);
    let execution_info = transaction.execute(&mut state, &context)?;

    Ok(execution_info)
}

/// Fetches all information needed to execute a given transaction
///
/// Due to limitations in the CachedState, we need to fetch this information
/// separately, and can't be done with only the CachedState
///
/// It doesn't use the rpc cache. See `fetch_transaction_w_state` to specify a custom reader.
pub fn fetch_transaction(
    hash: &TransactionHash,
    block_number: BlockNumber,
    chain: RpcChain,
    flags: ExecutionFlags,
) -> anyhow::Result<(BlockiTransaction, BlockContext)> {
    let reader = RpcStateReader::new(chain, block_number);
    let transaction = fetch_blockifier_transaction(&reader, flags, *hash)?;
    let context = fetch_block_context(&reader)?;

    Ok((transaction, context))
}

/// Fetches all information needed to execute a given transaction
///
/// Like `fetch_transaction`, but with a custom reader.
pub fn fetch_transaction_with_state(
    reader: &impl StateReader,
    hash: &TransactionHash,
    flags: ExecutionFlags,
) -> anyhow::Result<(BlockiTransaction, BlockContext)> {
    let transaction = fetch_blockifier_transaction(reader, flags, *hash)?;
    let context = fetch_block_context(reader)?;

    Ok((transaction, context))
}

/// Derives `BlockInfo` from the `BlockHeader`
pub fn get_block_info(header: BlockHeader) -> BlockInfo {
    fn parse_gas_price(price: GasPrice) -> NonzeroGasPrice {
        NonzeroGasPrice::new(price).unwrap_or(NonzeroGasPrice::MIN)
    }

    BlockInfo {
        block_number: header.block_number,
        sequencer_address: header.sequencer_address,
        block_timestamp: header.timestamp,
        gas_prices: validated_gas_prices(
            parse_gas_price(header.l1_gas_price.price_in_wei),
            parse_gas_price(header.l1_gas_price.price_in_fri),
            parse_gas_price(header.l1_data_gas_price.price_in_wei),
            parse_gas_price(header.l1_data_gas_price.price_in_fri),
            NonzeroGasPrice::MIN,
            NonzeroGasPrice::MIN,
        ),
        use_kzg_da: true,
    }
}

/// Derives `ClassInfo` from the `ContractClass`
pub fn get_class_info(class: ContractClass) -> anyhow::Result<ClassInfo> {
    match class {
        ContractClass::Sierra(sierra) => {
            let abi_length = sierra.abi.len();
            let sierra_length = sierra.sierra_program.len();
            let (contract_class, version) = sierra_to_versioned_contract_class_v1(sierra)?;
            Ok(ClassInfo::new(
                &contract_class,
                sierra_length,
                abi_length,
                version,
            )?)
        }
        ContractClass::Legacy(legacy) => {
            let abi_length = legacy
                .abi
                .clone()
                .expect("legendary contract should have abi")
                .len();
            Ok(ClassInfo::new(
                &legacy_to_contract_class_v0(legacy)?,
                0,
                abi_length,
                SierraVersion::DEPRECATED,
            )?)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        thread,
    };

    use blockifier::{
        execution::call_info::{CallInfo, ChargedResources, EventSummary, ExecutionSummary},
        fee::resources::{StarknetResources, StateResources},
        state::cached_state::StateChangesCount,
        transaction::account_transaction::ExecutionFlags,
    };
    use cairo_vm::{
        types::builtin_name::BuiltinName, vm::runners::cairo_runner::ExecutionResources,
    };
    use pretty_assertions_sorted::assert_eq_sorted;
    use starknet_api::{
        block::{BlockNumber, FeeType},
        class_hash,
        execution_resources::{GasAmount, GasVector},
        felt,
        state::StorageKey,
    };
    use test_case::test_case;

    use super::*;
    use crate::{objects::RpcCallInfo, reader::RpcChain};

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
        let hash = TransactionHash(felt!(hash));
        let block_number = BlockNumber(block_number);
        let flags = ExecutionFlags {
            only_query: false,
            charge_fee: false,
            validate: true,
        };

        let tx_info = execute_transaction(&hash, block_number, chain, flags).unwrap();

        let next_reader = RpcStateReader::new(chain, block_number);
        let trace = next_reader.get_transaction_trace(&hash).unwrap();

        assert_eq!(
            tx_info.revert_error.map(|r| r.to_string()),
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
        let hash = TransactionHash(felt!(hash));
        let block_number = BlockNumber(block_number);
        let flags = ExecutionFlags {
            only_query: false,
            charge_fee: false,
            validate: true,
        };

        let tx_info = execute_transaction(&hash, block_number, chain, flags).unwrap();

        let next_reader = RpcStateReader::new(chain, block_number);
        let trace = next_reader.get_transaction_trace(&hash).unwrap();

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
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(192), l2_gas: GasAmount(0) },
        7,
        3,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 2,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 1,
        },
        false,
        1,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 3035,
                    n_memory_holes: 74,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::pedersen, 4),
                        (BuiltinName::range_check, 75),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x816dd0297efc55dc1e7559020a3a825e81ef734b558f03c83325d4da7e6253"),
                class_hash!("0x4ad3c1dc8413453db314497945b6903e1c766495a1e60492d44da9c2a986e4b"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x558d16edafba7ea87e7b3e97642103a3b83511a8439f83d345c8cc1fb8c1cea"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x301976036fab639ccfd796c13864cd6988e6d0449b0ca749fd505b83d57fcc4"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x3779d024ee75b674955ff5025ec51faffd55610d2f586d2f7a4ce7b6b5d2463"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x301976036fab639ccfd796c13864cd6988e6d0449b0ca749fd505b83d57fcc4"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x282372af1ee63ce325edc788cde17330918ef2f8b9a33039b5bd8dcf192ec76"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x558d16edafba7ea87e7b3e97642103a3b83511a8439f83d345c8cc1fb8c1ceb"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x4531ecd0aefdd998d337d3dfc40af9bb3512edac441f2a31e0d499233086fec"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x4531ecd0aefdd998d337d3dfc40af9bb3512edac441f2a31e0d499233086feb"
                        ),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 1,
                total_event_keys: 1,
                total_event_data_size: 4,
            },
        }
    )]
    #[test_case(
        "0x0355059efee7a38ba1fd5aef13d261914608dce7bdfacad92a71e396f0ad7a77",
        661815,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(320), l2_gas: GasAmount(0) },
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
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 3457,
                    n_memory_holes: 26,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::pedersen, 4),
                        (BuiltinName::range_check, 74),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x309c042d3729173c7f2f91a34f04d8c509c1b292d334679ef1aabf8da0899cc"),
                class_hash!("0x4ad3c1dc8413453db314497945b6903e1c766495a1e60492d44da9c2a986e4b"),
                class_hash!("0x3530cc4759d78042f1b543bf797f5f3d647cde0388c33734cf91b7f7b9314a9"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!(
                            "0x269ea391a9c99cb6cee43ff589169f547cbc48d7554fdfbbfa7f97f516da700"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0xf920571b9f85bdd92a867cfdc73319d0f8836f0e69e06e4c5566b6203f75cc"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x4009513ce606f0ec0973e1bd057b7a36ac14f1a203569e30713140e5c928dad"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x4009513ce606f0ec0973e1bd057b7a36ac14f1a203569e30713140e5c928dac"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x5460e3ada357c552adbe9fdd830aabe59b7a3b43284240fb606458be8c6e0a"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x5460e3ada357c552adbe9fdd830aabe59b7a3b43284240fb606458be8c6e09"
                        ),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 2,
                total_event_keys: 2,
                total_event_data_size: 7,
            },
        }
    )]
    #[test_case(
        "0x05324bac55fb9fb53e738195c2dcc1e7fed1334b6db824665e3e984293bec95e",
        662246,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(320), l2_gas: GasAmount(0) },
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
        false,
        1,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 3457,
                    n_memory_holes: 26,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::pedersen, 4),
                        (BuiltinName::range_check, 74),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x25ec026985a3bf9d0cc1fe17326b245dfdc3ff89b8fde106542a3ea56c5a918"),
                class_hash!("0x33434ad846cdd5f23eb73ff09fe6fddd568284a0fb7d1be20ee482f044dabe2"),
                class_hash!("0x4ad3c1dc8413453db314497945b6903e1c766495a1e60492d44da9c2a986e4b"),
                ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x15ce587ff19a4baa941545deb4359e7d29b8ea3b224829a514425adbc5371d3"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x15ce587ff19a4baa941545deb4359e7d29b8ea3b224829a514425adbc5371d2"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x3aeff2c4fa75aace8f3974aa291ed288c2946cb2c89d3d45f43ec2e3d341266"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x3aeff2c4fa75aace8f3974aa291ed288c2946cb2c89d3d45f43ec2e3d341267"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x69a7818562b608ce8c5d0039e7f6d1c6ee55f36978f633b151858d85c022d2f"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0xf920571b9f85bdd92a867cfdc73319d0f8836f0e69e06e4c5566b6203f75cc"
                        ),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 2,
                total_event_keys: 2,
                total_event_data_size: 7,
            },
        }
    )]
    #[test_case(
        "0x670321c71835004fcab639e871ef402bb807351d126ccc4d93075ff2c31519d",
        654001,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(320), l2_gas: GasAmount(0) },
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
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 3938,
                    n_memory_holes: 63,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::pedersen, 4),
                        (BuiltinName::range_check, 76),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x1a736d6ed154502257f02b1ccdf4d9d1089f80811cd6acad48e6b6a9d1f2003"),
                class_hash!("0x5ffbcfeb50d200a0677c48a129a11245a3fc519d1d98d76882d1c9a1b19c6ed"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!(
                            "0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x6e73f308bbb7e97d64ea61262979f73ade2cad94f7d2a8b9c4a9d6debbf27ef"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x547f8114927592559689f3723f98f995af7f74cbda36db122195f71cf3c2693"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x547f8114927592559689f3723f98f995af7f74cbda36db122195f71cf3c2692"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x6e73f308bbb7e97d64ea61262979f73ade2cad94f7d2a8b9c4a9d6debbf27ee"
                        ),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 2,
                total_event_keys: 3,
                total_event_data_size: 7,
            },
        }
    )]
    #[test_case(
        "0x06962f11a96849ebf05cd222313858a93a8c5f300493ed6c5859dd44f5f2b4e3",
        654770,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(320), l2_gas: GasAmount(0) },
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
        false,
        1,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 3411,
                    n_memory_holes: 30,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::pedersen, 4),
                        (BuiltinName::range_check, 76),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x1a736d6ed154502257f02b1ccdf4d9d1089f80811cd6acad48e6b6a9d1f2003"),
                class_hash!("0x4ad3c1dc8413453db314497945b6903e1c766495a1e60492d44da9c2a986e4b"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0xb1c927d569cd996d8f1c1b677ccee5bc7fe8e0f97f706d1cce28f5fd17b44d"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0xb1c927d569cd996d8f1c1b677ccee5bc7fe8e0f97f706d1cce28f5fd17b44e"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!(
                            "0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"
                        ),
                    ),
                    StorageKey(
                        patricia_key!("0x26e662f47f90b15254e3da267636a7893da2164e73385b47acdf2605d70350b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
                    ),
                    StorageKey(
                        patricia_key!("0x26e662f47f90b15254e3da267636a7893da2164e73385b47acdf2605d70350c"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 2,
                total_event_keys: 3,
                total_event_data_size: 7,
            },
        }
    )]
    #[test_case(
        "0x078b81326882ecd2dc6c5f844527c3f33e0cdb52701ded7b1aa4d220c5264f72",
        653019,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(640), l2_gas: GasAmount(0) },
        28,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 7,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 3,
        },
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 45076,
                    n_memory_holes: 1809,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::pedersen, 39),
                        (BuiltinName::range_check, 1435),
                        (BuiltinName::segment_arena, 2),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x4f9849485e35f4a1c57d69b297feda94e743151f788202a6d731173babf4aec"),
                class_hash!("0x7f3777c99f3700505ea966676aac4a0d692c2a9f5e667f4c606b51ca1dd3420"),
                class_hash!("0x7b33a07ec099c227130ddffc9d74ad813fbcb8e0ff1c0f3ce097958e3dfc70b"),
                class_hash!("0x816dd0297efc55dc1e7559020a3a825e81ef734b558f03c83325d4da7e6253"),
                class_hash!("0x5ffbcfeb50d200a0677c48a129a11245a3fc519d1d98d76882d1c9a1b19c6ed"),
                class_hash!("0x4ac055f14361bb6f7bf4b9af6e96ca68825e6037e9bdf87ea0b2c641dea73ae"),
                class_hash!("0x182dfcf12cf38789f5937a1b920f0513195131a408716224ac8273f371d9d0a"),
                class_hash!("0x5ee939756c1a60b029c594da00e637bf5923bf04a86ff163e877e899c0840eb"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x27171399f13c5078064c25dff40dadf2762e5c393b7154209964e7a80e04863"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x2c40cfaa9c0aeba40ea0b8f5818e1a12c44c5e9c01c31beb8fd21f5dab2f95e"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x27171399f13c5078064c25dff40dadf2762e5c393b7154209964e7a80e04864"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5a2c887b4759dcad85a9164f219f88ac0098570bf6dbb934fb24f95fc45220c"),
                    ),
                    StorageKey(
                        patricia_key!("0x3779d024ee75b674955ff5025ec51faffd55610d2f586d2f7a4ce7b6b5d2463"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x27171399f13c5078064c25dff40dadf2762e5c393b7154209964e7a80e04864"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x2a8ae95dddffb0c09b9e1d7dacea66d3d564bd00c190cd4c9660a7e8a555fb0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5ef8800d242c5d5e218605d6a10e81449529d4144185f95bf4b8fb669424516"),
                    ),
                    StorageKey(
                        patricia_key!("0x261d7b045ffcfe9aad8e2d16a5e9195fdd1ff58e84f6019048cf342c79501b2"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4270219d365d6b017231b52e92b3fb5d7c8378b05e9abc97724537a80e93b0f"),
                    ),
                    StorageKey(
                        patricia_key!("0xc157281c325105632605b5874203b6a26582ef61f9507a569aa5d2a7637cd7"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x493875a3926558b908441a8fd6642a9f5b85f7fc5e39289c3a83b72b2eca837"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x27171399f13c5078064c25dff40dadf2762e5c393b7154209964e7a80e04863"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4270219d365d6b017231b52e92b3fb5d7c8378b05e9abc97724537a80e93b0f"),
                    ),
                    StorageKey(
                        patricia_key!("0x2232b2e1d8025562b381e10d97d2aa29b0ba14ab2fb65abaf36ea6179ff1067"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5ef8800d242c5d5e218605d6a10e81449529d4144185f95bf4b8fb669424516"),
                    ),
                    StorageKey(
                        patricia_key!("0x1f5dba4f0e386fe3e03022985e50076614214c29faad4f1a66fd553c39c47ed"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x493875a3926558b908441a8fd6642a9f5b85f7fc5e39289c3a83b72b2eca838"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4270219d365d6b017231b52e92b3fb5d7c8378b05e9abc97724537a80e93b0f"),
                    ),
                    StorageKey(
                        patricia_key!("0x2c98037748f28d346ea45305cb75d2b43b6a280f2bbf0a033fcdb7c23dacb94"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x1aa58c2130812a393a81be1c126ab39cc69de7d4cc9091535a3738bff6bca5a"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x2c40cfaa9c0aeba40ea0b8f5818e1a12c44c5e9c01c31beb8fd21f5dab2f95e"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x2c40cfaa9c0aeba40ea0b8f5818e1a12c44c5e9c01c31beb8fd21f5dab2f95d"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x29e4f0fb421255927c6fe4a10d0d56fb9cad419f02b4456a1cebd6da07fabbd"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x29e4f0fb421255927c6fe4a10d0d56fb9cad419f02b4456a1cebd6da07fabbe"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5ef8800d242c5d5e218605d6a10e81449529d4144185f95bf4b8fb669424516"),
                    ),
                    StorageKey(
                        patricia_key!("0x587f8a359f3afbadaac7e3a22b5d00fa5f08794c82353701e04afb0485d8c1"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4270219d365d6b017231b52e92b3fb5d7c8378b05e9abc97724537a80e93b0f"),
                    ),
                    StorageKey(
                        patricia_key!("0x2dd61772d8928d37542f9463273c969d38b38efa4c26744578f0890f1534d3d"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5ef8800d242c5d5e218605d6a10e81449529d4144185f95bf4b8fb669424516"),
                    ),
                    StorageKey(
                        patricia_key!("0x1bee0233b83cc233e905a16e35ad64d3720b430ffc95ac935ee81f1c2bb70a8"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5ef8800d242c5d5e218605d6a10e81449529d4144185f95bf4b8fb669424516"),
                    ),
                    StorageKey(
                        patricia_key!("0x3e9df762c67f04c3d19de6f877d7906e3a52e992c3f97013dc2450ab7851c9"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5ef8800d242c5d5e218605d6a10e81449529d4144185f95bf4b8fb669424516"),
                    ),
                    StorageKey(
                        patricia_key!("0x511270adbc9dd47783d90b9494051b71a2ca036eae8193fa3ea697266d1202"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5ef8800d242c5d5e218605d6a10e81449529d4144185f95bf4b8fb669424516"),
                    ),
                    StorageKey(
                        patricia_key!("0x1f5dba4f0e386fe3e03022985e50076614214c29faad4f1a66fd553c39c47ee"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x2c40cfaa9c0aeba40ea0b8f5818e1a12c44c5e9c01c31beb8fd21f5dab2f95d"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5a2c887b4759dcad85a9164f219f88ac0098570bf6dbb934fb24f95fc45220c"),
                    ),
                    StorageKey(
                        patricia_key!("0x282372af1ee63ce325edc788cde17330918ef2f8b9a33039b5bd8dcf192ec76"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5ef8800d242c5d5e218605d6a10e81449529d4144185f95bf4b8fb669424516"),
                    ),
                    StorageKey(
                        patricia_key!("0x3e9df762c67f04c3d19de6f877d7906e3a52e992c3f97013dc2450ab7851ca"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4270219d365d6b017231b52e92b3fb5d7c8378b05e9abc97724537a80e93b0f"),
                    ),
                    StorageKey(
                        patricia_key!("0x3f1abe37754ee6ca6d8dfa1036089f78a07ebe8f3b1e336cdbf3274d25becd0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x29e4f0fb421255927c6fe4a10d0d56fb9cad419f02b4456a1cebd6da07fabbe"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x29e4f0fb421255927c6fe4a10d0d56fb9cad419f02b4456a1cebd6da07fabbd"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x2a8ae95dddffb0c09b9e1d7dacea66d3d564bd00c190cd4c9660a7e8a555fb1"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x1aa58c2130812a393a81be1c126ab39cc69de7d4cc9091535a3738bff6bca5b"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 12,
                total_event_keys: 12,
                total_event_data_size: 58,
            },
        }
    )]
    #[test_case(
        "0x0780e3a498b4fd91ab458673891d3e8ee1453f9161f4bfcb93dd1e2c91c52e10",
        650558,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(448), l2_gas: GasAmount(0) },
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
        false,
        2,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 7394,
                    n_memory_holes: 222,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::range_check, 272),
                        (BuiltinName::pedersen, 26),
                        (BuiltinName::poseidon, 1)
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x5ffbcfeb50d200a0677c48a129a11245a3fc519d1d98d76882d1c9a1b19c6ed"),
                class_hash!("0x816dd0297efc55dc1e7559020a3a825e81ef734b558f03c83325d4da7e6253"),
                class_hash!("0x6312b8cc5222001e694fedc019c1160ff478ad6ae0fb066dc354b75bf9b5454"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x784cdb1e85de7d857e859b4ad93b0cf2e354f9612e74fc19a1c4d0f4cfc5c3c"),
                    ),
                    StorageKey(
                        patricia_key!("0x3779d024ee75b674955ff5025ec51faffd55610d2f586d2f7a4ce7b6b5d2463"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xc530f2c0aa4c16a0806365b0898499fba372e5df7a7172dc6fe9ba777e8007"),
                    ),
                    StorageKey(
                        patricia_key!("0x3e4e1993901faad3dd005be17839130abdccb4c36ab73f74dd3f05333e2b8ef"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x784cdb1e85de7d857e859b4ad93b0cf2e354f9612e74fc19a1c4d0f4cfc5c3c"),
                    ),
                    StorageKey(
                        patricia_key!("0x282372af1ee63ce325edc788cde17330918ef2f8b9a33039b5bd8dcf192ec76"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xc530f2c0aa4c16a0806365b0898499fba372e5df7a7172dc6fe9ba777e8007"),
                    ),
                    StorageKey(
                        patricia_key!("0x3e4e1993901faad3dd005be17839130abdccb4c36ab73f74dd3f05333e2b8f0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6e05b85d84a254faa07938b867b76aca1f1d95ebeb6bb44894c1d1912ec3180"),
                    ),
                    StorageKey(
                        patricia_key!("0x75f128ea43bc75b11cb0532e6873df6e9398b11d788ffbdd600a4546e83c10d"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6e05b85d84a254faa07938b867b76aca1f1d95ebeb6bb44894c1d1912ec3180"),
                    ),
                    StorageKey(
                        patricia_key!("0x6bd3a5ce5dcb97eaa770a5a3c9e4f5c752d99c61823a1d3d086688ee21a1247"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6e05b85d84a254faa07938b867b76aca1f1d95ebeb6bb44894c1d1912ec3180"),
                    ),
                    StorageKey(
                        patricia_key!("0xb59f37c0f9d09ea41ec01867728e0af61d0339b945f7d193fd07f4f96cfee8"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xc530f2c0aa4c16a0806365b0898499fba372e5df7a7172dc6fe9ba777e8007"),
                    ),
                    StorageKey(
                        patricia_key!("0x6e568983c06797c6e39843d87ff5e9ae88dc8bec182bbbe3936a32647fc9a1a"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xc530f2c0aa4c16a0806365b0898499fba372e5df7a7172dc6fe9ba777e8007"),
                    ),
                    StorageKey(
                        patricia_key!("0x6e568983c06797c6e39843d87ff5e9ae88dc8bec182bbbe3936a32647fc9a19"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6e05b85d84a254faa07938b867b76aca1f1d95ebeb6bb44894c1d1912ec3180"),
                    ),
                    StorageKey(
                        patricia_key!("0x1a22f545ba9d916b44403905200f55b36377a2e761b2184577679b0d7f7bc94"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 2,
                total_event_keys: 2,
                total_event_data_size: 6,
            },
        }
    )]
    #[test_case(
        "0x4f552c9430bd21ad300db56c8f4cae45d554a18fac20bf1703f180fac587d7e",
        351226,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(128), l2_gas: GasAmount(0) },
        3,
        0,
        0,
        Some(3),
        StateChangesCount {
            n_storage_updates: 2,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 0,
        },
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 8422,
                    n_memory_holes: 40,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::range_check, 161),
                        (BuiltinName::pedersen, 4),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x2760f25d5a4fb2bdde5f561fd0b44a3dee78c28903577d37d669939d97036a0"),
                class_hash!("0x4f5bce5f70bb1fcf6573f68205d3e74538c46c14dc47d37bdbde4c1abaf4e1e"),
                class_hash!("0xd0e183745e9dae3e4e78a8ffedcce0903fc4900beace4e0abf192d4c202da3"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x73314940630fd6dcda0d772d4c972c4e0a9946bef9dabf4ef84eda8ef542b82"),
                    ),
                    StorageKey(
                        patricia_key!("0x5d2e9527cbeb1a51aa084b0de7501f343b7b1bf24a0c427d6204a7b7988970"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x73314940630fd6dcda0d772d4c972c4e0a9946bef9dabf4ef84eda8ef542b82"),
                    ),
                    StorageKey(
                        patricia_key!("0xc88ee7a00e0b95f1138ef53d396c4327eeed7f9677bbd02ce82a663537b1cf"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x45ef7281f1485fd8d0298fc971ec7f2f8cf67d18b32a5e2cc876c957753332b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x5d2e9527cbeb1a51aa084b0de7501f343b7b1bf24a0c427d6204a7b7988970"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x110e2f729c9c2b988559994a3daccd838cf52faf88e18101373e67dd061455a"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x110e2f729c9c2b988559994a3daccd838cf52faf88e18101373e67dd061455b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x1390569bb0a3a722eb4228e8700301347da081211d5c2ded2db22ef389551ab"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x45ef7281f1485fd8d0298fc971ec7f2f8cf67d18b32a5e2cc876c957753332c"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x73314940630fd6dcda0d772d4c972c4e0a9946bef9dabf4ef84eda8ef542b82"),
                    ),
                    StorageKey(
                        patricia_key!("0x1dc79e2fd056704ede52dca5746b720269aaa5da53301dff546657c16ca07af"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 2,
                total_event_keys: 2,
                total_event_data_size: 7,
            },
        }
    )]
    #[test_case(
        "0x176a92e8df0128d47f24eebc17174363457a956fa233cc6a7f8561bfbd5023a",
        317093,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(128), l2_gas: GasAmount(0) },
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
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 2263,
                    n_memory_holes: 7,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::range_check, 38),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x1a736d6ed154502257f02b1ccdf4d9d1089f80811cd6acad48e6b6a9d1f2003"),
                class_hash!("0x2d49ae8a5475e2185e6044592f034e85011d53e29b527b4ea35aed4063d9e44"),
            ]),
            visited_storage_entries: HashSet::new(),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 2,
                total_event_keys: 3,
                total_event_data_size: 5,
            },
        }
    )]
    #[test_case(
        "0x026c17728b9cd08a061b1f17f08034eb70df58c1a96421e73ee6738ad258a94c",
        169929,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(128), l2_gas: GasAmount(0) },
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
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 2430,
                    n_memory_holes: 3,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::range_check, 39),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x33434ad846cdd5f23eb73ff09fe6fddd568284a0fb7d1be20ee482f044dabe2"),
                class_hash!("0x25ec026985a3bf9d0cc1fe17326b245dfdc3ff89b8fde106542a3ea56c5a918"),
                class_hash!("0x2d49ae8a5475e2185e6044592f034e85011d53e29b527b4ea35aed4063d9e44"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x68826cd135a7ae273d26bf0b93de662db2e7dd0a6f765327b77c98a5d3b600d"),
                    ),
                    StorageKey(
                        patricia_key!("0xf920571b9f85bdd92a867cfdc73319d0f8836f0e69e06e4c5566b6203f75cc"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 2,
                total_event_keys: 2,
                total_event_data_size: 5,
            },
        }
    )]
    #[test_case(
        "0x73ef9cde09f005ff6f411de510ecad4cdcf6c4d0dfc59137cff34a4fc74dfd",
        654001,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(128), l2_gas: GasAmount(0) },
        5,
        0,
        0,
        Some(5),
        StateChangesCount {
            n_storage_updates: 2,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 0,
        },
        false,
        1,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 3086,
                    n_memory_holes: 55,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::range_check, 72),
                        (BuiltinName::pedersen, 3),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x7f3777c99f3700505ea966676aac4a0d692c2a9f5e667f4c606b51ca1dd3420"),
                class_hash!("0x358663e6ed9d37efd33d4661e20b2bad143e0f92076b0c91fe65f31ccf55046"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x73314940630fd6dcda0d772d4c972c4e0a9946bef9dabf4ef84eda8ef542b82"),
                    ),
                    StorageKey(
                        patricia_key!("0x290e00617050a68193b715654df85cc41c3b79f263373d66459c8a3d5780b46"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x77e2b393d935021c9cd33f5a6869ff1742f4e87bc38cc5ed18f10c9eb7fe8d9"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x110e2f729c9c2b988559994a3daccd838cf52faf88e18101373e67dd061455b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x1390569bb0a3a722eb4228e8700301347da081211d5c2ded2db22ef389551ab"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x73314940630fd6dcda0d772d4c972c4e0a9946bef9dabf4ef84eda8ef542b82"),
                    ),
                    StorageKey(
                        patricia_key!("0xc88ee7a00e0b95f1138ef53d396c4327eeed7f9677bbd02ce82a663537b1cf"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x110e2f729c9c2b988559994a3daccd838cf52faf88e18101373e67dd061455a"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x77e2b393d935021c9cd33f5a6869ff1742f4e87bc38cc5ed18f10c9eb7fe8d8"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 2,
                total_event_keys: 4,
                total_event_data_size: 6,
            },
        }
    )]
    #[test_case(
        "0x0743092843086fa6d7f4a296a226ee23766b8acf16728aef7195ce5414dc4d84",
        186549,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(384), l2_gas: GasAmount(0) },
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
        false,
        1,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 39395,
                    n_memory_holes: 118,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::range_check, 1234),
                        (BuiltinName::pedersen, 9),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x4572af1cd59b8b91055ebb78df8f1d11c59f5270018b291366ba4585d4cdff0"),
                class_hash!("0x33434ad846cdd5f23eb73ff09fe6fddd568284a0fb7d1be20ee482f044dabe2"),
                class_hash!("0x7f7125c5958bf48de9d6a3ad045f845095d9572dc1a4b77da365f358c478cce"),
                class_hash!("0x25ec026985a3bf9d0cc1fe17326b245dfdc3ff89b8fde106542a3ea56c5a918"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x6a05844a03bb9e744479e3298f54705a35966ab04140d3d8dd797c1f6dc49d0"),
                    ),
                    StorageKey(
                        patricia_key!("0x3f1abe37754ee6ca6d8dfa1036089f78a07ebe8f3b1e336cdbf3274d25becd0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x8819d617fb205534ef545e2fcd15db7b2f43e98e65e242dad41ca4fe7d5256"),
                    ),
                    StorageKey(
                        patricia_key!("0xf920571b9f85bdd92a867cfdc73319d0f8836f0e69e06e4c5566b6203f75cc"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6a05844a03bb9e744479e3298f54705a35966ab04140d3d8dd797c1f6dc49d0"),
                    ),
                    StorageKey(
                        patricia_key!("0x28bbddb888b5f48fac1bfff91a9c86f45de0488d1418b204a4a77fddbf13d72"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6a05844a03bb9e744479e3298f54705a35966ab04140d3d8dd797c1f6dc49d0"),
                    ),
                    StorageKey(
                        patricia_key!("0x31b2db582739d919e49855d464d4ef21e6bfd0aa6db07d6f4cf006bf32ec7e8"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6a05844a03bb9e744479e3298f54705a35966ab04140d3d8dd797c1f6dc49d0"),
                    ),
                    StorageKey(
                        patricia_key!("0x110e2f729c9c2b988559994a3daccd838cf52faf88e18101373e67dd061455a"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6a05844a03bb9e744479e3298f54705a35966ab04140d3d8dd797c1f6dc49d0"),
                    ),
                    StorageKey(
                        patricia_key!("0x110e2f729c9c2b988559994a3daccd838cf52faf88e18101373e67dd061455b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6a05844a03bb9e744479e3298f54705a35966ab04140d3d8dd797c1f6dc49d0"),
                    ),
                    StorageKey(
                        patricia_key!("0x20ddb4628d547df787159e32a15efecfbfb960110c425c74e6b35421adca448"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6a05844a03bb9e744479e3298f54705a35966ab04140d3d8dd797c1f6dc49d0"),
                    ),
                    StorageKey(
                        patricia_key!("0x28bbddb888b5f48fac1bfff91a9c86f45de0488d1418b204a4a77fddbf13d73"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6a05844a03bb9e744479e3298f54705a35966ab04140d3d8dd797c1f6dc49d0"),
                    ),
                    StorageKey(
                        patricia_key!("0x64adb0d4dfddace954f8460d8327cb16ec952a7221eaab02a02294c5aad7a63"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 3,
                total_event_keys: 3,
                total_event_data_size: 10,
            },
        }
    )]
    #[test_case(
        "0x066e1f01420d8e433f6ef64309adb1a830e5af0ea67e3d935de273ca57b3ae5e",
        662252,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(448), l2_gas: GasAmount(0) },
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
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 30042,
                    n_memory_holes: 3584,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::range_check, 1369),
                        (BuiltinName::poseidon, 36),
                        (BuiltinName::pedersen, 3),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x589a40e9cd8784359c066db1adaf6cf0d92322ce579fc0c19739649beae132"),
                class_hash!("0x3c8904d062171ab62c1f2e52e2b33299c305626a8f8a253a1544a6ad774121b"),
                class_hash!("0x1a736d6ed154502257f02b1ccdf4d9d1089f80811cd6acad48e6b6a9d1f2003"),
                class_hash!("0x5dde112c893e2f5ed85b92a08d93cfa5579ce95d27afb34e47b7e7aad59c1c0"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x12ed0a68687678217e8e212e851aaaf26f24b745382184bac5b8f83e2089d09"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x1161aad19d349dc80ba5fb3bf52b82512f548b6631d4fc936347426e587f612"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x67ac2c2c96afe3312ea11413b2fdbf75b345d6f37dc435bc3e3fc2178780165"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x6c876061653ded49c5f4b2be528a17c0b99c707c36010da0dfdf7e86c88ed14"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x64cc83b312a847fec159095793816da6b9251610ebd55497d7e8fdbe0db5b8c"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x67ac2c2c96afe3312ea11413b2fdbf75b345d6f37dc435bc3e3fc2178780166"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x3eaa3e4be55bc8ada64d23f168b178d714a85ad9971bf1d5ff6bd6b703775dc"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x67ac2c2c96afe3312ea11413b2fdbf75b345d6f37dc435bc3e3fc2178780164"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x5602256700ad140f3346bd762776cec457aaa1a4c6597607818faa0f1086386"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x64cc83b312a847fec159095793816da6b9251610ebd55497d7e8fdbe0db5b8d"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x4a7ca3168783dab075bc552295fca05ea8d02a2bdeb461946b10187ef4b122d"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x4a7ca3168783dab075bc552295fca05ea8d02a2bdeb461946b10187ef4b122c"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 5,
                total_event_keys: 10,
                total_event_data_size: 63,
            },
        }
    )]
    // Check this tx, l1_data_gas should be 384
    // https://starkscan.co/tx/0x04756d898323a8f884f5a6aabd6834677f4bbaeecc2522f18b3ae45b3f99cd1e
    #[test_case(
        "0x04756d898323a8f884f5a6aabd6834677f4bbaeecc2522f18b3ae45b3f99cd1e",
        662250,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(768), l2_gas: GasAmount(0) },
        10,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 10,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 8108,
                    n_memory_holes: 893,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::pedersen, 3),
                        (BuiltinName::range_check, 165),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x6551d5af6e4eb4fac2f9fea06948a49a6d12d924e43a63e6034a6a75e749577"),
                class_hash!("0x1a736d6ed154502257f02b1ccdf4d9d1089f80811cd6acad48e6b6a9d1f2003"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x235a4d83b9b27bde6943b4e26301642f73f6fb6e5cda141734b02962b9d7f9d"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x291625bbd3d00024377934a31b5cdf6dfcc1e76776985889e17efb47b3ce2f0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x1e44381b0f95a657a83284ece260c1948e6c965da4d357f7fbd6b557342cdc0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x7d42c1838d13fa94d3f00304fe618766ad0ada8e0138966e8161b47a92c7e69"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x144978e3c7c8da8f54b97b4fa49320e49d8c70be0c35f42ba1e78465949d19c"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 11,
                total_event_keys: 12,
                total_event_data_size: 38,
            },
        }
    )]
    #[test_case(
        "0x00f390691fd9e865f5aef9c7cc99889fb6c2038bc9b7e270e8a4fe224ccd404d",
        662251,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(384), l2_gas: GasAmount(0) },
        12,
        5,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 4,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false,
        2,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 8068,
                    n_memory_holes: 605,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::poseidon, 10),
                        (BuiltinName::range_check, 250),
                        (BuiltinName::pedersen, 1),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x36078334509b514626504edc9fb252328d1a240e4e948bef8d0c08dff45927f"),
                class_hash!("0x5dde112c893e2f5ed85b92a08d93cfa5579ce95d27afb34e47b7e7aad59c1c0"),
                class_hash!("0x7992c30dd1dc4ce93b44e37e3c48b37635ca31f16c8518e88e15ff5686face"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x79cd4284246c43115a8376850d14c1f78570cde561a096ad209a50f653d722f"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x5046041b2f3c48f2f3b7ed44f3d4233fd85427a20ac2dd9e7b23da551e06d2b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x6164114d6518ebb3e8846b99c34c706b00f7bb475b3c5b8f67f85d336527162"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x79cd4284246c43115a8376850d14c1f78570cde561a096ad209a50f653d7231"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x1e637c300d7c105df1f620c34f08a99b0757f177f2d9d52a7a6d6a337f5cad6"),
                    ),
                    StorageKey(
                        patricia_key!("0x587f8a359f3afbadaac7e3a22b5d00fa5f08794c82353701e04afb0485d8c1"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x12ed0a68687678217e8e212e851aaaf26f24b745382184bac5b8f83e2089d09"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x79cd4284246c43115a8376850d14c1f78570cde561a096ad209a50f653d7230"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x422d33a3638dcc4c62e72e1d6942cd31eb643ef596ccac2351e0e21f6cd4bf4"),
                    ),
                    StorageKey(
                        patricia_key!("0x3117bb5a3b11d2eb8edb4980b76562d25525d1fcfa8f4873c0a2737a8e05ab6"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 4,
                total_event_keys: 6,
                total_event_data_size: 17,
            },
        }
    )]
    #[test_case(
        "0x26be3e906db66973de1ca5eec1ddb4f30e3087dbdce9560778937071c3d3a83",
        351269,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(128), l2_gas: GasAmount(0) },
        3,
        0,
        0,
        Some(3),
        StateChangesCount {
            n_storage_updates: 2,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 0,
        },
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 8414,
                    n_memory_holes: 44,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::pedersen, 4),
                        (BuiltinName::range_check, 161),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0xd0e183745e9dae3e4e78a8ffedcce0903fc4900beace4e0abf192d4c202da3"),
                class_hash!("0x2760f25d5a4fb2bdde5f561fd0b44a3dee78c28903577d37d669939d97036a0"),
                class_hash!("0x4f5bce5f70bb1fcf6573f68205d3e74538c46c14dc47d37bdbde4c1abaf4e1e"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x5d2e9527cbeb1a51aa084b0de7501f343b7b1bf24a0c427d6204a7b7988970"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x73314940630fd6dcda0d772d4c972c4e0a9946bef9dabf4ef84eda8ef542b82"),
                    ),
                    StorageKey(
                        patricia_key!("0xc88ee7a00e0b95f1138ef53d396c4327eeed7f9677bbd02ce82a663537b1cf"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x110e2f729c9c2b988559994a3daccd838cf52faf88e18101373e67dd061455b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x1390569bb0a3a722eb4228e8700301347da081211d5c2ded2db22ef389551ab"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x73314940630fd6dcda0d772d4c972c4e0a9946bef9dabf4ef84eda8ef542b82"),
                    ),
                    StorageKey(
                        patricia_key!("0x5d2e9527cbeb1a51aa084b0de7501f343b7b1bf24a0c427d6204a7b7988970"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x1b9e3d38adfc69623b8d12acf32adfde1319fb40263927c8fc287371bc2d571"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x110e2f729c9c2b988559994a3daccd838cf52faf88e18101373e67dd061455a"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x49d36570d4e46f48e99674bd3fcc84644ddd6b96f7c741b1562b82f9e004dc7"),
                    ),
                    StorageKey(
                        patricia_key!("0x1b9e3d38adfc69623b8d12acf32adfde1319fb40263927c8fc287371bc2d570"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x73314940630fd6dcda0d772d4c972c4e0a9946bef9dabf4ef84eda8ef542b82"),
                    ),
                    StorageKey(
                        patricia_key!("0x1dc79e2fd056704ede52dca5746b720269aaa5da53301dff546657c16ca07af"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 2,
                total_event_keys: 2,
                total_event_data_size: 7,
            },
        }
    )]
    #[test_case(
        "0x0310c46edc795c82c71f600159fa9e6c6540cb294df9d156f685bfe62b31a5f4",
        662249,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(1088), l2_gas: GasAmount(0) },
        37,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 13,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 4,
        },
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 63180,
                    n_memory_holes: 8239,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::bitwise, 84),
                        (BuiltinName::pedersen, 79),
                        (BuiltinName::range_check, 4897),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x5fdb47de4edfd5983f6a82a1beeb1aab2b69d3674b90730aa1693d94d73f0d3"),
                class_hash!("0x27067191f94e301b528b1624e7998d51606412522aab9621fb2c1899b983eeb"),
                class_hash!("0x29927c8af6bccf3f6fda035981e765a7bdbf18a2dc0d630494f8758aa908e2b"),
                class_hash!("0x3e8d67c8817de7a2185d418e88d321c89772a9722b752c6fe097192114621be"),
                class_hash!("0x4ad3c1dc8413453db314497945b6903e1c766495a1e60492d44da9c2a986e4b"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x40dff85b22ceb4de5477d6577f334c914f8efefed9f5c892934c0f6e966ed7"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x174ca4d40ded1b191eeaafb8c222368024fdf0ac894af16ecb51b7ec8477901"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
                    ),
                    StorageKey(
                        patricia_key!("0x3a8d0fa789f88e7211b601b97145655c736104895afee9fba450449b3abed8e"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x3251cfd36c995a7f163e76498e247f8b2d61aeda83a746bd61d4e7da5fe5699"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x100000001"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x43e4f09c32d13d43a880e85f69f7de93ceda62d6cf2581a582c6db635548fdc"),
                    ),
                    StorageKey(
                        patricia_key!("0x60648f61481391ac1803a92eaa5009bc1438ef6e8d10598d265bf5829d312f5"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
                    ),
                    StorageKey(
                        patricia_key!("0x43e1ef374bc5f9e49c6c9764a9aac6e36bc8e3df0ca3bffb3cde5a0990ca369"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
                    ),
                    StorageKey(
                        patricia_key!("0x586729208869bc7ed12aa50c7a1834a1eef579d05efcc17ee1217a1722f9ba3"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
                    ),
                    StorageKey(
                        patricia_key!("0x4dbfe544ed63377b3d819a142e245f56921729b4c3e205f87e66b64af007236"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x3110bae189a1cca17a6da4ee778f0076e1ea99cefa11a9b532fe7c064deb948"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x45fb8ffc8b8d0d2521fdc1b3558fea0c43ed03911387994698ae9e504026d44"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
                    ),
                    StorageKey(
                        patricia_key!("0x3a8d0fa789f88e7211b601b97145655c736104895afee9fba450449b3abed8f"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x40dff85b22ceb4de5477d6577f334c914f8efefed9f5c892934c0f6e966ed6"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x1"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
                    ),
                    StorageKey(
                        patricia_key!("0x586729208869bc7ed12aa50c7a1834a1eef579d05efcc17ee1217a1722f9ba4"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x4428af60ea017cd8ac5a041f3777f8b2d7450f8474e471bcbae608cca7975ae"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x356fd6aa989033f329e6a5aceb143ba7d0667fd7f74fbbf9457080dd9352c0b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x43e4f09c32d13d43a880e85f69f7de93ceda62d6cf2581a582c6db635548fdc"),
                    ),
                    StorageKey(
                        patricia_key!("0x281a85306374a5ab27f0bbc385296a54bcd314a1948b6cf61c4ea1bc44bb9f8"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x73ffd08441115d89e71906f927d21e18c97a612c56116b40280b8ee6ae5e1f6"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x100000000"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
                    ),
                    StorageKey(
                        patricia_key!("0x4dbfe544ed63377b3d819a142e245f56921729b4c3e205f87e66b64af007235"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x6aff514d3c14935bb84bfc2f5e443daa68c6b9af02150bce2904dc5cee89fb4"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x43e4f09c32d13d43a880e85f69f7de93ceda62d6cf2581a582c6db635548fdc"),
                    ),
                    StorageKey(
                        patricia_key!("0x60648f61481391ac1803a92eaa5009bc1438ef6e8d10598d265bf5829d312f4"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4718f5a0fc34cc1af16a1cdee98ffb20c31f5cd61d6ab07201858f4287c938d"),
                    ),
                    StorageKey(
                        patricia_key!("0x43e1ef374bc5f9e49c6c9764a9aac6e36bc8e3df0ca3bffb3cde5a0990ca36a"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x68153282240797844a90a332ec1b2eabf7d15c42ba228e0c249720e6914176f"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x4505a9f06f2bd639b6601f37a4dc0908bb70e8e0e0c34b1220827d64f4fc066"),
                    ),
                    StorageKey(
                        patricia_key!("0x281a85306374a5ab27f0bbc385296a54bcd314a1948b6cf61c4ea1bc44bb9f8"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x6267108c93bc853ce42efb890b0793116c50e5a98b13e024640da0d272115a8"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0x45fb8ffc8b8d0d2521fdc1b3558fea0c43ed03911387994698ae9e504026d43"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5dd3d2f4429af886cd1a3b08289dbcea99a294197e9eb43b0e0325b4b"),
                    ),
                    StorageKey(
                        patricia_key!("0xd5ee72ce4ea000d334dd7d37bd432beb488d5ceae18a4a955e14901a851ec7"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 13,
                total_event_keys: 14,
                total_event_data_size: 121,
            },
        }
    )]
    #[test_case(
        "0x06a09ffbf996178ac6e90101047e42fe29cb7108573b2ecf4b0ebd2cba544cb4",
        662248,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(896), l2_gas: GasAmount(0) },
        4,
        2,
        0,
        None,
        StateChangesCount {
            n_storage_updates: 12,
            n_class_hash_updates: 0,
            n_compiled_class_hash_updates: 0,
            n_modified_contracts: 2,
        },
        false,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 3730,
                    n_memory_holes: 100,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::range_check, 78),
                        (BuiltinName::pedersen, 3),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x6551d5af6e4eb4fac2f9fea06948a49a6d12d924e43a63e6034a6a75e749577"),
                class_hash!("0x816dd0297efc55dc1e7559020a3a825e81ef734b558f03c83325d4da7e6253"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x401109fe98501f72c01ea79ad5b48c227ec2706d706723cfec3fa5d4ba54c90"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x291625bbd3d00024377934a31b5cdf6dfcc1e76776985889e17efb47b3ce2f0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x144978e3c7c8da8f54b97b4fa49320e49d8c70be0c35f42ba1e78465949d19c"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xce305af75e878121f5691150fdd386dff5af073f7e56f4882f62e02bb05a45"),
                    ),
                    StorageKey(
                        patricia_key!("0x282372af1ee63ce325edc788cde17330918ef2f8b9a33039b5bd8dcf192ec76"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xce305af75e878121f5691150fdd386dff5af073f7e56f4882f62e02bb05a45"),
                    ),
                    StorageKey(
                        patricia_key!("0x3779d024ee75b674955ff5025ec51faffd55610d2f586d2f7a4ce7b6b5d2463"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x1e44381b0f95a657a83284ece260c1948e6c965da4d357f7fbd6b557342cdc0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x3db6ec55ed007baefa72e1ff639054f691cb3009b778edb59a37aa04690682c"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x6182278e63816ff4080ed07d668f991df6773fd13db0ea10971096033411b11"),
                    ),
                    StorageKey(
                        patricia_key!("0x235a4d83b9b27bde6943b4e26301642f73f6fb6e5cda141734b02962b9d7f9d"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 7,
                total_event_keys: 7,
                total_event_data_size: 26,
            },
        }
    )]
    #[test_case(
        "0x026e04e96ba1b75bfd066c8e138e17717ecb654909e6ac24007b644ac23e4b47",
        536893,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(896), l2_gas: GasAmount(0) },
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
        false,
        1,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 60919,
                    n_memory_holes: 975,
                    builtin_instance_counter: HashMap::from_iter([
                        (BuiltinName::range_check, 1715),
                        (BuiltinName::pedersen, 43),
                    ]),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x5ad7636491f8a6b210c137e6191bc12daf31171b6c6e670fe1387697810403a"),
                class_hash!("0xdb2ed00ec7872d4d093f3acb479d7ef2b56bcc7fc793707c1fb04ec66c6b10"),
                class_hash!("0x1a736d6ed154502257f02b1ccdf4d9d1089f80811cd6acad48e6b6a9d1f2003"),
                class_hash!("0x26fe8ea36ec7703569cfe4693b05102940bf122647c4dbf0abc0bb919ce27bd"),
                class_hash!("0x4c2a0ab0698957bd725745ff6dfdda0e874ade8681770fe8990016a6fa9cf04"),
                class_hash!("0x2b39bc3f4c1fd5bef8b7d21504c44e0da59cf27b350551b13d913da52e40d3b"),
                class_hash!("0x2580632f25bbee45da5a1a1ef49d03a984f78dde4019069aa9f25aac06f941a"),
                class_hash!("0x7b5cd6a6949cc1730f89d795f2442f6ab431ea6c9a5be00685d50f97433c5eb"),
                class_hash!("0x52c7ba99c77fc38dd3346beea6c0753c3471f2e3135af5bb837d6c9523fff62"),
                class_hash!("0x7197021c108b0cc57ae354f5ad02222c4b3d7344664e6dd602a0e2298595434"),
                class_hash!("0x64e7d628b1b2aa04a35fe6610b005689e8b591058f7f92bf4eb234e67cf403b"),
                class_hash!("0x2760f25d5a4fb2bdde5f561fd0b44a3dee78c28903577d37d669939d97036a0"),
            ]),
            visited_storage_entries: HashSet::from_iter([
                (
                    ContractAddress(
                        patricia_key!("0xdad44c139a476c7a17fc8141e6db680e9abc9f56fe249a105094c44382c2fd"),
                    ),
                    StorageKey(
                        patricia_key!("0x3f1abe37754ee6ca6d8dfa1036089f78a07ebe8f3b1e336cdbf3274d25becd0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x3b3a699bb6ef37ff4b9c4e14319c7d8e9c9bdd10ff402d1ebde18c62ae58382"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x6f4db3b80b3f05685fbb3cbcb6dc5a676c638925269901e02993ca6c121fa4f"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x1c76cd4f3f79786d9e5d1298f47170de4bf0222337c680c5377ec772d3ce96b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x3351bce4793f90e4aa00447357c2d34ac08611756193d8249009e0396dd7b41"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x34fea4b03b28c51f5bea6524bf41d06209d8306dc2376d4d730b14fea79ec8c"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x1c76cd4f3f79786d9e5d1298f47170de4bf0222337c680c5377ec772d3ce96b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x2f91121a0e63b29dc1d6a4afc3a3963209345391a124869e657665e749659ad"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x2f91121a0e63b29dc1d6a4afc3a3963209345391a124869e657665e749659ac"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x3b3a699bb6ef37ff4b9c4e14319c7d8e9c9bdd10ff402d1ebde18c62ae58381"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x6f4db3b80b3f05685fbb3cbcb6dc5a676c638925269901e02993ca6c121fa50"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x7dd6927869f0f6501f10407d6de416a90d2272af189f8b24d12acc09c2df5e6"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xdad44c139a476c7a17fc8141e6db680e9abc9f56fe249a105094c44382c2fd"),
                    ),
                    StorageKey(
                        patricia_key!("0x5c722fd91dec49f67eb170afb8e7e54ebe6f35b6b7f4fec3fdd0bad96606126"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x6f4db3b80b3f05685fbb3cbcb6dc5a676c638925269901e02993ca6c121fa4f"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x5a7a0269b464f3a77a50c4de3ef3ba7e2c253c514f440ef75786d28d59007d9"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xf6f4cf62e3c010e0ac2451cc7807b5eec19a40b0faacd00cca3914280fdf5a"),
                    ),
                    StorageKey(
                        patricia_key!("0x119407278bb67ccb5306f7b08343c96f7b6933a06f3173067d21e98725a2b59"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x5a7a0269b464f3a77a50c4de3ef3ba7e2c253c514f440ef75786d28d59007d8"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x41fd22b238fa21cfcf5dd45a8548974d8263b3a531a60388411c5e230f97023"),
                    ),
                    StorageKey(
                        patricia_key!("0x139825585c3389ee852d93d2706b57d5bf8d4ba85922ef0689a691627211b05"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x2ec31623436b0fbcbbd71b2b3eed2887c8156077098a52a9256a9e0edb833f"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x2f91121a0e63b29dc1d6a4afc3a3963209345391a124869e657665e749659ac"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x1e6f3e4333da349f86a03f030be7f2c76d8266a97c625746ebb9d3220a39d87"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x57c60b189063035eed65879a14ad5f6e718027a212dafbe52f9bcd79e9f4fb"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x6f4db3b80b3f05685fbb3cbcb6dc5a676c638925269901e02993ca6c121fa50"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xf6f4cf62e3c010e0ac2451cc7807b5eec19a40b0faacd00cca3914280fdf5a"),
                    ),
                    StorageKey(
                        patricia_key!("0x2ded64caa8ae4aba3291cc1f172d4b8fda206d4d1b5660c7565e7461c727929"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x12f8e318fe04a1fe8bffe005ea4bbd19cb77a656b4f42682aab8a0ed20702f0"),
                    ),
                    StorageKey(
                        patricia_key!("0x5d6c55cbfbeaef29110e02a1f4a1228488feec6f59b3f2f77d24245272e799a"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x18d7b079b471cfde70d671fc55b3c06d4363cdc22201c09924822902a33e01a"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x5a7a0269b464f3a77a50c4de3ef3ba7e2c253c514f440ef75786d28d59007d9"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x41fd22b238fa21cfcf5dd45a8548974d8263b3a531a60388411c5e230f97023"),
                    ),
                    StorageKey(
                        patricia_key!("0x3f1abe37754ee6ca6d8dfa1036089f78a07ebe8f3b1e336cdbf3274d25becd0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x18d7b079b471cfde70d671fc55b3c06d4363cdc22201c09924822902a33e019"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x57c60b189063035eed65879a14ad5f6e718027a212dafbe52f9bcd79e9f4fa"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x3ad6945aa849b77ba309199d76ea4555be65368391ba450fab8ea6248431ea0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xf6f4cf62e3c010e0ac2451cc7807b5eec19a40b0faacd00cca3914280fdf5a"),
                    ),
                    StorageKey(
                        patricia_key!("0x1f7cb1620ff9ec226d1c4c618ea516ebdd5a65005c2f457158cdd0d4d77ab4b"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x2ec31623436b0fbcbbd71b2b3eed2887c8156077098a52a9256a9e0edb8340"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x53c91253bc9682c04929ca02ed00b3e423f6710d2ee7e0d5ebb06f3ecf368a8"),
                    ),
                    StorageKey(
                        patricia_key!("0x5a7a0269b464f3a77a50c4de3ef3ba7e2c253c514f440ef75786d28d59007d8"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x3f1abe37754ee6ca6d8dfa1036089f78a07ebe8f3b1e336cdbf3274d25becd0"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x37d931bc5c74214b7c669bc431e9dc2c39cba57516e0ab3c5a7ceb2d1c9e5c1"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x3ad6945aa849b77ba309199d76ea4555be65368391ba450fab8ea6248431e9f"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!("0x35bfc9b032421f93a52912870bbb09af6c896d21d00165845f0c04715df1468"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x7dd6927869f0f6501f10407d6de416a90d2272af189f8b24d12acc09c2df5e5"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x68f5c6a61780768455de69077e07e89787839bf8166decfbf92b645209c0fb8"),
                    ),
                    StorageKey(
                        patricia_key!("0x2f91121a0e63b29dc1d6a4afc3a3963209345391a124869e657665e749659ad"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0xf6f4cf62e3c010e0ac2451cc7807b5eec19a40b0faacd00cca3914280fdf5a"),
                    ),
                    StorageKey(
                        patricia_key!("0x26be8966924075afd93cd2a84013cca637468173cc02904002dc3d0caf30b61"),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x5801bdad32f343035fb242e98d1e9371ae85bc1543962fedea16c59b35bd19b"),
                    ),
                    StorageKey(
                        patricia_key!(
                            "0x1e6f3e4333da349f86a03f030be7f2c76d8266a97c625746ebb9d3220a39d88"
                        ),
                    ),
                ),
                (
                    ContractAddress(
                        patricia_key!("0x12f8e318fe04a1fe8bffe005ea4bbd19cb77a656b4f42682aab8a0ed20702f0"),
                    ),
                    StorageKey(
                        patricia_key!("0x5d6c55cbfbeaef29110e02a1f4a1228488feec6f59b3f2f77d24245272e7999"),
                    ),
                ),
            ]),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 11,
                total_event_keys: 12,
                total_event_data_size: 54,
            },
        }
    )]
    #[test_case(
        "0x01351387ef63fd6fe5ec10fa57df9e006b2450b8c68d7eec8cfc7d220abc7eda",
        644700,
        RpcChain::MainNet,
        GasVector { l1_gas: GasAmount(0), l1_data_gas: GasAmount(128), l2_gas: GasAmount(0) },
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
        true,
        0,
        ExecutionSummary {
            charged_resources: ChargedResources {
                vm_resources: ExecutionResources {
                    n_steps: 28,
                    n_memory_holes: 0,
                    builtin_instance_counter: HashMap::new(),
                },
                gas_consumed: GasAmount(
                    0,
                ),
            },
            executed_class_hashes: HashSet::from_iter([
                class_hash!("0x33478650b3b71be225cbad55fda8a590022eea17be3212d0ccbf3d364b1e448"),
            ]),
            visited_storage_entries: HashSet::new(),
            l2_to_l1_payload_lengths: vec![],
            event_summary: EventSummary {
                n_events: 0,
                total_event_keys: 0,
                total_event_data_size: 0,
            },
        }
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
        n_allocated_keys: usize,
        execution_summary: ExecutionSummary,
    ) {
        let hash = TransactionHash(felt!(hash));
        let block_number = BlockNumber(block_number);
        let flags = ExecutionFlags {
            only_query: false,
            charge_fee: false,
            validate: true,
        };
        let tx_info = execute_transaction(&hash, block_number, chain, flags).unwrap();

        let starknet_resources = tx_info.clone().receipt.resources.starknet_resources;
        let state_resources = StateResources::new_for_testing(starknet_chg, n_allocated_keys);
        let starknet_rsc = StarknetResources::new(
            calldata_length,
            signature_length,
            code_size,
            state_resources,
            l1_handler_payload_size,
            execution_summary,
        );
        assert_eq_sorted!(is_reverted, tx_info.revert_error.is_some());
        assert_eq_sorted!(da_gas, tx_info.receipt.da_gas);
        assert_eq_sorted!(starknet_rsc, starknet_resources);
    }

    #[test_case(
        "0x0310c46edc795c82c71f600159fa9e6c6540cb294df9d156f685bfe62b31a5f4",
        662249,
        RpcChain::MainNet
    )]
    #[test_case(
        "0x066e1f01420d8e433f6ef64309adb1a830e5af0ea67e3d935de273ca57b3ae5e",
        662252,
        RpcChain::MainNet
    )]
    /// Ideally contract executions should be independent from one another.
    /// In practice we use the same loaded dynamic shared library for each
    /// execution of the same contract, for performance reasons. This means that
    /// if a contract relies on global variables, those will be shared between
    /// different executions of the same contract. This test executes a single
    /// transaction (therefore, the same contracts) multiple times at the same
    /// time, helping to uncover any possible concurrency bug that we may have
    fn test_concurrency(tx_hash: &str, block_number: u64, chain: RpcChain) {
        let hash = TransactionHash(felt!(tx_hash));
        let block_number = BlockNumber(block_number);
        let flags = ExecutionFlags {
            only_query: false,
            charge_fee: false,
            validate: true,
        };
        let (tx, context) = fetch_transaction(&hash, block_number, chain, flags).unwrap();

        let mut handles = Vec::new();

        for _ in 0..20 {
            let context = context.clone();
            let tx = tx.clone();

            let previous_block_number = block_number.prev().unwrap();
            let current_reader = RpcStateReader::new(chain, previous_block_number);
            let mut cache = CachedState::new(current_reader);

            handles.push(thread::spawn(move || {
                let execution_info = tx.execute(&mut cache, &context).unwrap();

                assert!(
                    !execution_info.is_reverted(),
                    "{:?}",
                    execution_info.revert_error.unwrap()
                )
            }));
        }

        for h in handles {
            h.join().unwrap()
        }
    }

    // Impl conversion for easier checking against RPC data
    impl From<&CallInfo> for RpcCallInfo {
        fn from(value: &CallInfo) -> Self {
            Self {
                result: Some(value.execution.retdata.0.clone()),
                calldata: Some((*value.call.calldata.0).clone()),
                calls: value.inner_calls.iter().map(|ci| ci.into()).collect(),
                // We don't have the revert reason string in the trace so we just make sure it doesn't revert
                revert_reason: value.execution.failed.then_some("Default String".into()),
            }
        }
    }

    #[test]
    fn test_get_block_info() {
        let reader = RpcStateReader::new(RpcChain::MainNet, BlockNumber(169928));

        let block = reader.get_block_with_tx_hashes().unwrap();
        let info = get_block_info(block.header);

        assert_eq!(
            info.gas_prices.l1_gas_price(&FeeType::Eth).get().0,
            22804578690
        );
    }
}
