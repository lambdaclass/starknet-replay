#[derive(Drop, PartialEq, Debug, starknet::Event)]
pub struct MerkleTreeEvent {
    pub root: felt252,
}
