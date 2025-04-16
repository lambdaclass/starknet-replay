use std::fs::File;

use cairo_lang_starknet_classes::contract_class::ContractClass;

fn main() {
    let mut args = std::env::args();
    args.next();

    let contract_path = args
        .next()
        .expect("expected contract path as first argument");
    let contract_file = File::open(contract_path).expect("failed to open contract file");

    let contract: ContractClass =
        serde_json::from_reader(contract_file).expect("failed to parse contract");

    let sierra = contract
        .extract_sierra_program()
        .expect("failed to extract sierra program");

    print!("{}", sierra);
}
