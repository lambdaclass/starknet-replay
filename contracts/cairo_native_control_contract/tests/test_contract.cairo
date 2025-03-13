use cairo_native_control_contract::{
    IMerkleTreeDispatcher, IMerkleTreeDispatcherTrait, IMerkleTreeSafeDispatcher,
    IMerkleTreeSafeDispatcherTrait, Proof, IntegerHasherImpl
};
use core::hash::{HashStateExTrait, HashStateTrait};
use core::poseidon::PoseidonTrait;
use snforge_std::{ContractClassTrait, DeclareResultTrait, declare};
use starknet::ContractAddress;

fn deploy_contract(name: ByteArray) -> ContractAddress {
    let contract = declare(name).unwrap().contract_class();
    let (contract_address, _) = contract.deploy(@ArrayTrait::new()).unwrap();
    contract_address
}

#[test]
fn test_create_merkle_tree() {
    let contract_address = deploy_contract("CairoNativeControl");

    let dispatcher = IMerkleTreeDispatcher { contract_address };

    let array = array![1, 2, 3, 4];

    let merkle_tree = dispatcher.create_new_tree(array);

    let h1 = PoseidonTrait::new().update_with(1).finalize();
    let h2 = PoseidonTrait::new().update_with(2).finalize();
    let h3 = PoseidonTrait::new().update_with(3).finalize();
    let h4 = PoseidonTrait::new().update_with(4).finalize();

    let h12 = PoseidonTrait::new().update_with((h1, h2)).finalize();
    let h34 = PoseidonTrait::new().update_with((h3, h4)).finalize();

    let h1234 = PoseidonTrait::new().update_with((h12, h34)).finalize();

    let merkle_tree_expected = array![h1, h2, h3, h4, h12, h34, h1234];

    assert_eq!(merkle_tree_expected, merkle_tree);
}

#[test]
#[should_panic(expected: 'Data length is not power of 2')]
fn test_invalid_legth() {
    let contract_address = deploy_contract("CairoNativeControl");

    let dispatcher = IMerkleTreeDispatcher { contract_address };

    let array = array![1, 2, 3, 4, 5];

    dispatcher.create_new_tree(array);
}

#[test]
fn test_generate_proof_verify() {
    let contract_address = deploy_contract("CairoNativeControl");

    let dispatcher = IMerkleTreeDispatcher { contract_address };

    let array = array![1, 2, 3, 4, 5, 6, 7, 8];

    dispatcher.create_new_tree(array);

    let proof = dispatcher.generate_proof(1);
    
    assert!(dispatcher.verify(proof));
}

#[test]
fn test_wrong_proof_verify() {
    let contract_address = deploy_contract("CairoNativeControl");

    let dispatcher = IMerkleTreeDispatcher { contract_address };

    let array = array![1, 2, 3, 4];

    dispatcher.create_new_tree(array);

    let h1 = PoseidonTrait::new().update_with(1).finalize();
    let h2 = PoseidonTrait::new().update_with(2).finalize();

    let h12 = PoseidonTrait::new().update_with((h1, h2)).finalize();
    
    let wrong_proof = Proof { data: 1, index: 0, hashes: array![h1, h12, h2]};
    
    assert!(!dispatcher.verify(wrong_proof));
}

#[test]
#[should_panic(expected: 'Invalid input prove')]
fn test_input_proof_verify() {
    let contract_address = deploy_contract("CairoNativeControl");

    let dispatcher = IMerkleTreeDispatcher { contract_address };

    let array = array![1, 2, 3, 4];

    dispatcher.create_new_tree(array);
    
    dispatcher.generate_proof(6);
}
