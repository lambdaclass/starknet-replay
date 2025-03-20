#[generate_trait]
pub impl IntegerHasherImpl of IntegerHasher {
    fn to_hash(self: i32) -> felt252 {
        let mut serialize_array = ArrayTrait::new();

        self.serialize(ref serialize_array);

        core::poseidon::poseidon_hash_span(serialize_array.span())
    }
}
