pub mod cache;
pub mod execution;
pub mod objects;
pub mod reader;
pub mod utils;

#[cfg(test)]
mod tests {
    use blockifier::state::state_api::StateReader;
    use pretty_assertions_sorted::{assert_eq, assert_eq_sorted};
    use starknet_api::{
        block::BlockNumber,
        class_hash,
        core::{ContractAddress, Nonce},
        felt,
        hash::StarkHash,
        patricia_key,
        state::StorageKey,
        transaction::TransactionHash,
    };

    use crate::reader::*;

    /// A utility macro to create a [`ContractAddress`] from a hex string / unsigned integer
    /// representation.
    /// Imported from starknet_api
    macro_rules! contract_address {
        ($s:expr) => {
            ContractAddress(patricia_key!($s))
        };
    }

    #[test]
    fn test_get_contract_class_cairo1() {
        let rpc_state = RpcStateReader::new(RpcChain::MainNet, BlockNumber(700000));

        let class_hash =
            class_hash!("0298e56befa6d1446b86ed5b900a9ba51fd2faa683cd6f50e8f833c0fb847216");
        // This belongs to
        // https://starkscan.co/class/0x0298e56befa6d1446b86ed5b900a9ba51fd2faa683cd6f50e8f833c0fb847216
        // which is cairo1.0

        rpc_state.get_contract_class(&class_hash).unwrap();
    }

    #[test]
    fn test_get_contract_class_cairo0() {
        let rpc_state = RpcStateReader::new(RpcChain::MainNet, BlockNumber(700000));

        let class_hash =
            class_hash!("025ec026985a3bf9d0cc1fe17326b245dfdc3ff89b8fde106542a3ea56c5a918");
        rpc_state.get_contract_class(&class_hash).unwrap();
    }

    #[test]
    fn test_get_class_hash_at() {
        let rpc_state = RpcStateReader::new(RpcChain::MainNet, BlockNumber(700000));
        let address =
            contract_address!("00b081f7ba1efc6fe98770b09a827ae373ef2baa6116b3d2a0bf5154136573a9");

        assert_eq!(
            rpc_state.get_class_hash_at(address).unwrap(),
            class_hash!("025ec026985a3bf9d0cc1fe17326b245dfdc3ff89b8fde106542a3ea56c5a918")
        );
    }

    #[test]
    fn test_get_nonce_at() {
        let rpc_state = RpcStateReader::new(RpcChain::TestNet, BlockNumber(700000));
        // Contract deployed by xqft which will not be used again, so nonce changes will not break
        // this test.
        let address =
            contract_address!("07185f2a350edcc7ea072888edb4507247de23e710cbd56084c356d265626bea");
        assert_eq!(
            rpc_state.get_nonce_at(address).unwrap(),
            Nonce(felt!("0x0")),
        );
    }

    #[test]
    fn test_get_storage_at() {
        let rpc_state = RpcStateReader::new(RpcChain::MainNet, BlockNumber(700000));
        let address =
            contract_address!("00b081f7ba1efc6fe98770b09a827ae373ef2baa6116b3d2a0bf5154136573a9");
        let key = StorageKey(patricia_key!(0u128));

        assert_eq_sorted!(
            rpc_state.get_storage_at(address, key).unwrap(),
            StarkHash::from_hex("0x0").unwrap()
        );
    }

    #[test]
    fn test_get_transaction() {
        let rpc_state = RpcStateReader::new(RpcChain::MainNet, BlockNumber(700000));
        let tx_hash = TransactionHash(
            StarkHash::from_hex("06da92cfbdceac5e5e94a1f40772d6c79d34f011815606742658559ec77b6955")
                .unwrap(),
        );

        rpc_state.get_transaction(&tx_hash).unwrap();
    }

    // Tested with the following query to the Feeder Gateway API:
    // https://alpha-mainnet.starknet.io/feeder_gateway/get_transaction_trace?transactionHash=0x035673e42bd485ae699c538d8502f730d1137545b22a64c094ecdaf86c59e592
    #[test]
    fn test_get_transaction_trace() {
        let rpc_state = RpcStateReader::new(RpcChain::MainNet, BlockNumber(700000));

        let tx_hash = TransactionHash(
            StarkHash::from_hex(
                "0x035673e42bd485ae699c538d8502f730d1137545b22a64c094ecdaf86c59e592",
            )
            .unwrap(),
        );

        let tx_trace = rpc_state.get_transaction_trace(&tx_hash).unwrap();

        assert_eq!(
            tx_trace.validate_invocation.as_ref().unwrap().calldata,
            Some(vec![
                StarkHash::from_dec_str("1").unwrap(),
                StarkHash::from_hex(
                    "0x45dc42889b6292c540de9def0341364bd60c2d8ccced459fac8b1bfc24fa1f5"
                )
                .unwrap(),
                StarkHash::from_hex(
                    "0xb758361d5e84380ef1e632f89d8e76a8677dbc3f4b93a4f9d75d2a6048f312"
                )
                .unwrap(),
                StarkHash::from_hex("0").unwrap(),
                StarkHash::from_hex("0xa").unwrap(),
                StarkHash::from_hex("0xa").unwrap(),
                StarkHash::from_hex("0x3fed4").unwrap(),
                StarkHash::from_hex("0").unwrap(),
                StarkHash::from_hex("0xdf6aedb").unwrap(),
                StarkHash::from_hex("0").unwrap(),
                StarkHash::from_hex("0").unwrap(),
                StarkHash::from_hex("0").unwrap(),
                StarkHash::from_hex(
                    "0x47c5f10d564f1623566b940a61fe54754bfff996f7536901ec969b12874f87f"
                )
                .unwrap(),
                StarkHash::from_hex("2").unwrap(),
                StarkHash::from_hex(
                    "0x72034953cd93dc8618123b4802003bae1f469b526bc18355250080c0f93dc17"
                )
                .unwrap(),
                StarkHash::from_hex(
                    "0x5f2ac628fa43d58fb8a6b7a2739de5c1edb550cb13cdcec5bc99f00135066a7"
                )
                .unwrap(),
            ])
        );
        assert_eq!(
            tx_trace.validate_invocation.as_ref().unwrap().result,
            Some(vec![])
        );
        assert_eq!(
            tx_trace.validate_invocation.as_ref().unwrap().calls.len(),
            1
        );

        assert_eq!(
            tx_trace.execute_invocation.as_ref().unwrap().calldata,
            Some(vec![
                StarkHash::from_hex("0x1").unwrap(),
                StarkHash::from_hex(
                    "0x45dc42889b6292c540de9def0341364bd60c2d8ccced459fac8b1bfc24fa1f5"
                )
                .unwrap(),
                StarkHash::from_hex(
                    "0xb758361d5e84380ef1e632f89d8e76a8677dbc3f4b93a4f9d75d2a6048f312"
                )
                .unwrap(),
                StarkHash::from_hex("0x0").unwrap(),
                StarkHash::from_hex("0xa").unwrap(),
                StarkHash::from_hex("0xa").unwrap(),
                StarkHash::from_hex("0x3fed4").unwrap(),
                StarkHash::from_hex("0x0").unwrap(),
                StarkHash::from_hex("0xdf6aedb").unwrap(),
                StarkHash::from_hex("0x0").unwrap(),
                StarkHash::from_hex("0x0").unwrap(),
                StarkHash::from_hex("0x0").unwrap(),
                StarkHash::from_hex(
                    "0x47c5f10d564f1623566b940a61fe54754bfff996f7536901ec969b12874f87f"
                )
                .unwrap(),
                StarkHash::from_hex("0x2").unwrap(),
                StarkHash::from_hex(
                    "0x72034953cd93dc8618123b4802003bae1f469b526bc18355250080c0f93dc17"
                )
                .unwrap(),
                StarkHash::from_hex(
                    "0x5f2ac628fa43d58fb8a6b7a2739de5c1edb550cb13cdcec5bc99f00135066a7"
                )
                .unwrap()
            ])
        );
        assert_eq!(
            tx_trace.execute_invocation.as_ref().unwrap().result,
            Some(vec![0u128.into()])
        );
        assert_eq!(tx_trace.execute_invocation.as_ref().unwrap().calls.len(), 1);
        assert_eq!(
            tx_trace.execute_invocation.as_ref().unwrap().calls[0]
                .calls
                .len(),
            1
        );
        assert_eq!(
            tx_trace.execute_invocation.as_ref().unwrap().calls[0].calls[0]
                .calls
                .len(),
            0
        );

        assert_eq!(
            tx_trace.fee_transfer_invocation.as_ref().unwrap().calldata,
            Some(vec![
                StarkHash::from_hex(
                    "0x1176a1bd84444c89232ec27754698e5d2e7e1a7f1539f12027f28b23ec9f3d8"
                )
                .unwrap(),
                StarkHash::from_hex("0x2439e47667460").unwrap(),
                StarkHash::from_hex("0").unwrap(),
            ])
        );
        assert_eq!(
            tx_trace.fee_transfer_invocation.as_ref().unwrap().result,
            Some(vec![1u128.into()])
        );
        assert_eq!(
            tx_trace
                .fee_transfer_invocation
                .as_ref()
                .unwrap()
                .calls
                .len(),
            1
        );
    }

    #[test]
    fn test_get_transaction_receipt() {
        let rpc_state = RpcStateReader::new(RpcChain::MainNet, BlockNumber(700000));
        let tx_hash = TransactionHash(
            StarkHash::from_hex("06da92cfbdceac5e5e94a1f40772d6c79d34f011815606742658559ec77b6955")
                .unwrap(),
        );

        rpc_state.get_transaction_receipt(&tx_hash).unwrap();
    }
}
