# Starknet Profiling

This crate provides an example on how to process samples for a transaction replay with cairo native.

## Obtaining a Profile

First, build the replay crate:

```bash
cargo build --release --features benchmark,profiling
```

Then, benchmark a transaction:

```bash
samply record --  target/release/replay bench-tx ...
```

You can obtain a sample profile at: https://share.firefox.dev/3H8dGXU.

To make the transaction more accurate, we suggest executing the transaction many times (with the `number_of_runs` arguments). The sample profile contains the execution of a single transaction 100k times.

## Processing a Profile

To process the samples, run:
```bash
cargo run --example cairo-native sample.json
```

It will output two different representation of the same data.

The first tree groups the calls by shared library.
```
│ GROUP BY SHARED LIBRARY
│ -----------------------
│ RATIO │  TOTAL  │  SELF   │ TREE
│       │         │         │
│ 100.0 │ 109588  │ 78953   │ replay
│ 21.0  │ 22961   │ 22961   │ ├─ libsystem_kernel.dylib
│ 4.6   │ 5005    │ 4481    │ ├─ MLIR
│ 0.3   │ 299     │ 259     │ │  ├─ libsystem_malloc.dylib
│ 0.0   │ 38      │ 38      │ │  │  ├─ libsystem_platform.dylib
│ 0.0   │ 2       │ 2       │ │  │  └─ libsystem_kernel.dylib
│ 0.2   │ 224     │ 224     │ │  ├─ libcompiler_rt.dylib
│ 0.0   │ 1       │ 1       │ │  └─ libdyld.dylib
│ 1.1   │ 1249    │ 1033    │ ├─ libsystem_malloc.dylib
│ 0.1   │ 128     │ 128     │ │  ├─ libsystem_platform.dylib
│ 0.1   │ 88      │ 88      │ │  └─ libsystem_kernel.dylib
│ 0.9   │ 1015    │ 1       │ ├─ libsystem_c.dylib
│ 0.9   │ 1014    │ 1014    │ │  └─ libsystem_kernel.dylib
│ 0.3   │ 338     │ 338     │ ├─ libsystem_platform.dylib
│ 0.0   │ 54      │ 54      │ ├─ dyld
│ 0.0   │ 13      │ 13      │ └─ libdyld.dylib
```

The second tree does more advanced processing, grouping by: blockifier, sierra, and runtime functions.
```
│ GROUP BY SYMBOL
│ ---------------
│ RATIO │  TOTAL  │  SELF   │ TREE
│       │         │         │
│ 100.0 │ 84798   │ 4585    │ blockifier
│ 94.6  │ 80213   │ 5005    │ └─ sierra
│ 35.9  │ 30425   │ 30425   │    ├─ ...cairo_native__libfunc__ec__ec_state_init
│ 18.6  │ 15777   │ 15777   │    ├─ ...cairo_native__libfunc__ec__ec_state_add_mul
│ 18.2  │ 15422   │ 15422   │    ├─ ...cairo_native__libfunc__ec__ec_point_from_x_nz
│ 8.5   │ 7180    │ 7180    │    ├─ ...cairo_native__libfunc__pedersen
│ 5.3   │ 4483    │ 4483    │    ├─ ...cairo_native__libfunc__hades_permutation
│ 1.4   │ 1145    │ 1145    │    ├─ ...cairo_native__libfunc__ec__ec_state_try_finalize_nz
│ 0.6   │ 526     │ 526     │    ├─ ...cairo_native__libfunc__ec__ec_state_add
│ 0.1   │ 72      │ 72      │    ├─ ...handler::StarknetSyscallHandlerCallbacks<T>::wrap_emit_event
│ 0.0   │ 40      │ 40      │    ├─ ...cairo_native__libfunc__ec__ec_point_try_new_nz
│ 0.0   │ 36      │ 36      │    ├─ ...handler::StarknetSyscallHandlerCallbacks<T>::wrap_storage_read
│ 0.0   │ 26      │ 26      │    ├─ ...handler::StarknetSyscallHandlerCallbacks<T>::wrap_get_execution_info_v2
│ 0.0   │ 19      │ 19      │    ├─ ...handler::StarknetSyscallHandlerCallbacks<T>::wrap_get_execution_info
│ 0.0   │ 18      │ 18      │    ├─ ...handler::StarknetSyscallHandlerCallbacks<T>::wrap_call_contract
│ 0.0   │ 13      │ 13      │    ├─ ...handler::StarknetSyscallHandlerCallbacks<T>::wrap_library_call
│ 0.0   │ 9       │ 9       │    ├─ ...cairo_native__dict_get
│ 0.0   │ 6       │ 6       │    ├─ ...cairo_native__get_costs_builtin
│ 0.0   │ 5       │ 5       │    ├─ ...handler::StarknetSyscallHandlerCallbacks<T>::wrap_storage_write
│ 0.0   │ 5       │ 5       │    ├─ ...cairo_native__dict_drop
│ 0.0   │ 1       │ 1       │    └─ ...handler::StarknetSyscallHandlerCallbacks<T>::wrap_get_block_hash
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
