# Starknet Profiling

This crate provides an example on how to process samples for a transaction replay with cairo native.

## Usage

To process the samples, run:
```bash
cargo run --example cairo-native sample.json
```

It will output different views of the same data. For example, the "Samples by Source" section will differentiate samples between: MLIR, Runtime, and Blockifier logic.

```
=================
Samples by Source
=================

84135  - 100  % - total
  75252  - 89.44% - Runtime
    30424  - 36.16% - cairo_native__libfunc__ec__ec_state_init
    15777  - 18.75% - cairo_native__libfunc__ec__ec_state_add_mul
    15422  - 18.33% - cairo_native__libfunc__ec__ec_point_from_x_nz
    7180   - 8.53 % - cairo_native__libfunc__pedersen
    4483   - 5.33 % - cairo_native__libfunc__hades_permutation
    1145   - 1.36 % - cairo_native__libfunc__ec__ec_state_try_finalize_nz
    526    - 0.63 % - cairo_native__libfunc__ec__ec_state_add
    82     - 0.1  % - wrap_emit_event
    59     - 0.07 % - wrap_storage_read
    40     - 0.05 % - cairo_native__libfunc__ec__ec_point_try_new_nz
    27     - 0.03 % - wrap_get_execution_info_v2
    22     - 0.03 % - wrap_get_execution_info
    20     - 0.02 % - wrap_call_contract
    18     - 0.02 % - wrap_library_call
    9      - 0.01 % - cairo_native__dict_get
    6      - 0.01 % - wrap_storage_write
    6      - 0.01 % - cairo_native__get_costs_builtin
    5      - 0.01 % - cairo_native__dict_drop
    1      - 0    % - wrap_get_block_hash
  5005   - 5.95 % - MLIR
  3598   - 4.28 % - blockifier
  279    - 0.33 % - cairo_native
  1      - 0    % - unknown
```

The section "Samples by Crate" is much simpler, and only differentiates between rust crates. MLIR execution is contained within "blockifier":

```
================
Samples by Crate
================

86468  - 100  % - total
  75677  - 87.52% - lambdaworks_math
  6971   - 8.06 % - blockifier
  2951   - 3.41 % - starknet_types_core
  557    - 0.64 % - cairo_native
  156    - 0.18 % - starknet_api
  155    - 0.18 % - lambdaworks_crypto
  1      - 0    % - unknown
```

# Understanding The Execution Flow

The execution flow of a transaction can be visualized as:

```
┌────────┐  ┌───────────┐  ┌───────┐   ┌────────┐
│ replay │─>│ sequencer │─>│ 0X... │─┬>│ native │
└────────┘  └───────────┘  └───────┘ │ └────────┘
                                     │ ┌────────┐  ┌───────────┐
                                     └>│ native │─>│ sequencer │ ...
                                       └────────┘  └───────────┘
```

- `replay`: We don't care for samples from this module.
- `sequencer`: It may represent different things, depending on when it was called:
  - The first sequencer box represents initial transaction validation and setup.
  - The inner sequencer boxes represnet syscall, which include fetching storage, or calling an inner contract.
- `0x`: Represents sierra code that was compiled to Native code.
- `native`: It may represent different things, depending on when it was called.
  - If it was called right after sierra code, it represents runtime or syscall handler execution.
  - If it was called right after blockifier/replay, it represents contract compilation or executor loading.

Given a sample, we can determine the crate it belong to by looking at the symbol name. For example, given the symbol name: `foo::bar::baz`, we know that we are execution function `baz` in crate `foo`.

For the case of sierra contracts, we can't rely on symbol names, but instead rely on the library the current address corresponds to. In the case of our replay, these libraries contain the prefix "0x".

With these knowledge, we can analyze each sample and determine where it belong to, building a report like the ones exampled above.
