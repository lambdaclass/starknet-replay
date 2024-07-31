use std::u128::{self, MAX};

use blockifier::state::{cached_state::CachedState, state_api::StateReader};
use lazy_static::lazy_static;
use rpc_state_reader::blockifier_state_reader::RpcStateReader;
use starknet_api::{core::ContractAddress, hash::StarkFelt, transaction::{DeclareTransaction, DeclareTransactionV2, Fee, TransactionSignature}};
use utils::load_contract;

lazy_static! {
    static ref ACCOUNT_ADDRESS: StarkFelt = StarkFelt::from_u128(4321);
    static ref OWNER_ADDRESS: StarkFelt = StarkFelt::from_u128(4321);
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    Ok(())
}

fn declear_erc20(mut state: &mut CachedState<RpcStateReader>) -> Result<StarkFelt, Box<dyn std::error::Error>> {
    let contract = load_contract("name")?;
    let sender_address = ContractAddress::try_from(*ACCOUNT_ADDRESS)?;
    let nonce = state.get_nonce_at(sender_address);
    let max_fee: Fee = Fee(u128::MAX);
    let signature = TransactionSignature::default();
    let class_hash = state.get_class_hash_at();

    let tx  = DeclareTransaction::V2(DeclareTransactionV2 {});

    Ok(9.into())
}

mod utils {
    use std::{fs, path::Path};

    use starknet_api::state::ContractClass;

    const BENCH_YAS: &str = "bench/yas/";

    pub fn load_contract(name: &str) -> Result<(ContractClass), Box<dyn std::error::Error>> {
        let path = Path::new(BENCH_YAS)
            .join(name)
            .with_extension("sierra.json");
        let sierra_contract = serde_json::from_str::<ContractClass>(&fs::read_to_string(
            path.with_extension("sierra.json"),
        )?)?;
        let casm_contract = serde_json::from_str::<ContractClass>(&fs::read_to_string(
            path.with_extension("json"),
        )?)?;

        Ok(sierra_contract)
    }
    
    // pub fn get_state() -> Result<CachedState<RpcStateReader>, std::error::Error> {
    //     let state_reader = RpcStateReader::;
    //     let state = CachedState::new(state_reader);

    //     Ok(state)
    // }
}
