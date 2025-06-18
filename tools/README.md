# Utility Tools

This crate contains utility binaries for manipulating and executing Cairo programs.

## Contracts

Consider the following Cairo program:
```cairo
// path: contract.cairo
#[starknet::contract]
mod Fibonacci {
    #[storage]
    struct Storage {}

    #[external(v0)]
    fn fib(self: @ContractState, a: felt252, b: felt252, n: felt252) -> felt252 {
        match n {
            0 => a,
            _ => fib(self, b, a + b, n - 1),
        }
    }
}

fn main() -> felt252 {
    let a = 3;
    let b = 5;
    let c = a + b;
    return c;
}
```

Compile it to a starknet contract with:
```bash
cargo run --bin cairo-to-contract -- contract.cairo > contract.json
```

You can then extract the sierra program with:
```bash
cargo run --bin contract-to-sierra -- contract.json > contract.sierra
```

## Programs

Consider the following Cairo program:
```cairo
// path: program.cairo
fn main() -> felt252 {
    let a = 10;
    let b = 20;
    return a + b;
}
```

Compile it to a sierra json with:
```bash
cargo run --bin cairo-to-sierra-json program.cairo > program.json
```

You can then execute it with:
```bash
cargo run --bin cairo-native-sierra-run program.json
```

It will output:
```json
{
  "remaining_gas": null,
  "return_value": {
    "Felt252": "0x1e"
  },
  "builtin_stats": {
    "bitwise": 0,
    "ec_op": 0,
    "range_check": 0,
    "pedersen": 0,
    "poseidon": 0,
    "segment_arena": 0,
    "range_check_96": 0,
    "circuit_add": 0,
    "circuit_mul": 0
  }
}
```
