#[generate_trait]
pub impl IntegerHasherImpl of IntegerHasher {
    fn to_hash(self: i32) -> felt252 {
        let mut serialize_array = ArrayTrait::new();

        self.serialize(ref serialize_array);

        core::poseidon::poseidon_hash_span(serialize_array.span())
    }
}

#[starknet::interface]
pub trait IMerkleTree<TContractState> {
    fn create_new_tree(ref self: TContractState, data: Array<i32>) -> Array<felt252>;
    fn generate_proof(self: @TContractState, data: i32) -> Proof;
    fn verify(self: @TContractState, proof: Proof) -> bool;
}

#[derive(Drop, Serde, Debug, PartialEq)]
pub struct Proof {
    pub data: i32,
    pub hashes: Array<felt252>,
    pub index: u64,
}

mod errors {
    pub const INVALID_DATA_LENGTH: felt252 = 'Data length is not power of 2';
    pub const INVALID_PROOF_INPUT: felt252 = 'Invalid input prove';
}

#[starknet::contract]
mod CairoNativeControl {
    use core::hash::{HashStateExTrait, HashStateTrait};
    use core::poseidon::PoseidonTrait;
    use starknet::storage::{Map, MutableVecTrait, StoragePointerReadAccess, StorageMapReadAccess, StorageMapWriteAccess, Vec, VecTrait};
    use starknet::{ContractAddress, get_caller_address};
    use super::{IntegerHasher, Proof};

    #[storage]
    struct Storage {
        pub trees: Vec<Vec<felt252>>,
        // each caller address is associated to a tree through
        // the latter's index allocation
        pub caller_tree: Map<ContractAddress, u64>
    }

    #[abi(embed_v0)]
    impl MerkleTreeImpl of super::IMerkleTree<ContractState> {
        fn create_new_tree(ref self: ContractState, data: Array<i32>) -> Array<felt252> {
            let mut data_len = data.len();

            assert(data_len > 0 && (data_len & data_len - 1) == 0, super::errors::INVALID_DATA_LENGTH);

            let mut array_hashes = ArrayTrait::new();
            let mut offset = 0;

            for d in data.span() {
                array_hashes.append(d.to_hash());
            }

            while data_len > 1 {
                let mut i = 0;

                while i < data_len - 1 {
                    let hash_1 = *array_hashes.at(offset + i);
                    let hash_2 = *array_hashes.at(offset + i + 1);

                    let new_hash = PoseidonTrait::new().update_with((hash_1, hash_2)).finalize();

                    array_hashes.append(new_hash);
                    i += 2;
                }

                offset += data_len;
                data_len /= 2;
            }

            self.write_caller_index(self.trees.len());
            self.write_tree(self.read_caller_index(), array_hashes.span());

            array_hashes
        }

        fn generate_proof(self: @ContractState, data: i32) -> Proof {
            let caller_index = self.read_caller_index();
            let tree_index = self.find_leaf_index(caller_index, data.to_hash()).expect(super::errors::INVALID_PROOF_INPUT);
            let mut proof = array![];
            
            let mut data_len = self.tree_len(self.read_caller_index());
            let mut i = tree_index;

            while data_len > 1 {
                let data_hash =
                    if i % 2 == 0 {
                        self.tree_at(caller_index, i + 1)
                    } else {
                        self.tree_at(caller_index, i - 1)
                    };

                proof.append(data_hash);

                data_len /= 2;
                i = data_len + 1 + i;
            }

            Proof { data, index: tree_index, hashes: proof }
        }

        fn verify(self: @ContractState, mut proof: Proof) -> bool {
            let caller_index = self.read_caller_index();

            let mut curr_hash = proof.data.to_hash();
            let mut index = proof.index;

            while let Option::Some(p) = proof.hashes.pop_front() {
                curr_hash =
                    if index % 2 == 0 {
                        PoseidonTrait::new().update_with((curr_hash, p)).finalize()
                    } else {
                        PoseidonTrait::new().update_with((p, curr_hash)).finalize()
                    };
                
                index /= 2;
            }

            curr_hash == self.read_root(caller_index)
        }
    }

    #[generate_trait]
    impl InternalCallerIndexTreeImpl of InternalCallerIndexTree {
        fn read_caller_index(self: @ContractState) -> u64 {
            let caller = get_caller_address();
            
            StorageMapReadAccess::read(self.caller_tree, caller)
        }
        fn write_caller_index(ref self: ContractState, index: u64) {
            let caller = get_caller_address();
            
            StorageMapWriteAccess::write(self.caller_tree, caller, index)
        }
    }

    #[generate_trait]
    impl InternalTreeImpl of InternalTree {
        fn find_leaf_index(self: @ContractState, caller_index: u64, hash: felt252) -> Option<u64> {
            let mut index = 0;
            for i in 0..self.tree_len(caller_index) {
                if self.tree_at(caller_index, i) == hash {
                    return Option::Some(index);
                }
                index += 1;
            }
            Option::None
        }
        fn read_root(self: @ContractState, caller_index: u64) -> felt252 {
            let tree_len = self.tree_len(caller_index);
            self.tree_at(caller_index, tree_len - 1)
        }
        fn tree_at(self: @ContractState, caller_index: u64, tree_index: u64) -> felt252 {
            self.trees.at(caller_index).at(tree_index).read()
        }
        fn write_tree(ref self: ContractState, caller_index: u64, tree: Span<felt252>) {
            self.trees.allocate();

            for h in tree {
                self.trees.at(caller_index).push(*h);
            }
        }
        fn tree_len(self: @ContractState, index: u64) -> u64 {
            self.trees.at(index).len()
        }
    }
}
