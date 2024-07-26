use std::sync::Arc;

use blockifier::{
    context::{BlockContext, ChainInfo, TransactionContext},
    execution::{
        common_hints::ExecutionMode,
        entry_point::{CallEntryPoint, CallType, EntryPointExecutionContext},
    },
    state::cached_state::CachedState,
    transaction::objects::CurrentTransactionInfo,
    versioned_constants::VersionedConstants,
};
use cairo_vm::vm::runners::cairo_runner::ExecutionResources;
use rpc_state_reader::{blockifier_state_reader::RpcStateReader, rpc_state::RpcState};
use starknet_api::{
    core::{ClassHash, ContractAddress, EntryPointSelector, PatriciaKey},
    deprecated_contract_class::EntryPointType,
    hash::StarkFelt,
    transaction::Calldata,
};

fn main() {
    let class_hash = Some(ClassHash(
        StarkFelt::try_from("0x0731820e650cf7522d36f262b26f8ba0961a916ec647e14a167f95dfd385d83a")
            .unwrap(),
    ));

    let entry_point_selector = EntryPointSelector(
        StarkFelt::try_from("0x015543c3708653cda9d418b4ccd3be11368e40636c10c44b18cfe756b6d88b29")
            .unwrap(),
    );

    let storage_address = ContractAddress(
        PatriciaKey::try_from(
            StarkFelt::try_from(
                "0x0038925b0bcf4dce081042ca26a96300d9e181b910328db54a6c89e5451503f5",
            )
            .unwrap(),
        )
        .unwrap(),
    );

    let calldata = Calldata(Arc::new(vec![
        StarkFelt::try_from("0x0234e6202a14fc14fff9c3403579b99cec024fc8823e2ec99c517461058b2a04")
            .unwrap(),
        StarkFelt::try_from("0x0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap(),
        StarkFelt::try_from("0x0000000000000000000000000000000000000000000000008d8dadf544fc0000")
            .unwrap(),
        StarkFelt::try_from("0x0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap(),
        StarkFelt::try_from("0x0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap(),
        StarkFelt::try_from("0x0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap(),
        StarkFelt::try_from("0x0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap(),
        StarkFelt::try_from("0x00000000000000000000000000000000000000000000000000162f54f860acdb")
            .unwrap(),
        StarkFelt::try_from("0x0000000000000000000000000000000000000000000000000000000000000000")
            .unwrap(),
        StarkFelt::try_from("0x0000000000000000000000000000000000000000000000000000000000000001")
            .unwrap(),
    ]));

    let caller_address = ContractAddress(
        PatriciaKey::try_from(
            StarkFelt::try_from(
                "0x07051f0b3042662c9094813c0b87996c1df451ab39839757d804c8cd8cdaf2c5",
            )
            .unwrap(),
        )
        .unwrap(),
    );

    let code_address = Some(ContractAddress(
        PatriciaKey::try_from(
            StarkFelt::try_from(
                "0x0038925b0bcf4dce081042ca26a96300d9e181b910328db54a6c89e5451503f5",
            )
            .unwrap(),
        )
        .unwrap(),
    ));

    let execute_call = CallEntryPoint {
        entry_point_type: EntryPointType::External,
        entry_point_selector,
        calldata,
        class_hash,
        code_address,
        storage_address,
        caller_address,
        call_type: CallType::Call,
        initial_gas: u64::MAX,
    };

    let mut state = CachedState::new(RpcStateReader::new(
        RpcState::new_rpc(
            rpc_state_reader::rpc_state::RpcChain::MainNet,
            rpc_state_reader::rpc_state::BlockValue::Number(starknet_api::block::BlockNumber(
                626173,
            )),
        )
        .unwrap(),
    ));

    let rpc_block_info = state.state.0.get_block_info().unwrap();
    let gas_prices = state
        .state
        .0
        .get_gas_price(rpc_block_info.block_number.0)
        .unwrap();

    let mut resources = ExecutionResources::default();
    let mut context = EntryPointExecutionContext::new(
        Arc::new(TransactionContext {
            block_context: BlockContext::new_unchecked(
                &blockifier::blockifier::block::BlockInfo {
                    block_number: rpc_block_info.block_number,
                    block_timestamp: rpc_block_info.block_timestamp,
                    sequencer_address: rpc_block_info.sequencer_address,
                    gas_prices,
                    use_kzg_da: false,
                },
                &ChainInfo::default(),
                &VersionedConstants::latest_constants_with_overrides(u32::MAX, usize::MAX),
            ),
            tx_info: blockifier::transaction::objects::TransactionInfo::Current(
                CurrentTransactionInfo::default(),
            ),
        }),
        ExecutionMode::Execute,
        false,
    )
    .unwrap();

    let call_info = execute_call
        .execute(&mut state, &mut resources, &mut context)
        .unwrap();

    dbg!(call_info);
}
