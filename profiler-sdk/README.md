# Profiler SDK

This crate contains the type definitions and utilities for processing samples in the [processed profile format](https://github.com/firefox-devtools/profiler/blob/main/docs-developer/processed-profile-format.md). The crate is agnostic to the Starknet context.

In the future, it may be useful tu support other formats like [Gecko profile format](https://github.com/firefox-devtools/profiler/blob/main/docs-developer/gecko-profile-format.md), and [perf data format](https://git.kernel.org/pub/scm/linux/kernel/git/perf/perf-tools-next.git/tree/tools/perf/Documentation/perf.data-file-format.txt).

## Obtaining the Profile

First, you need a profile to process. You can generate one yourself by using `samply`:
```bash
samply record -- <command> <args>
```
You can also reuse the profile of someone else, like this one: https://share.firefox.dev/3H8dGXU. This sample profile was taken from the Samply repository.

Once the firefox profiler has loaded the profile, you can download it to a `json.gz` file, which can be uncompressed to the raw `.json` profile file.

## Processing a Profile

Once you have a profile, you can process it using the profiler-sdk. The `shared-libraries` example groups the calls by the shared library they belong to:

```bash
cargo run --example shared-libraries ~/drafts/dump-syms.jso
```

It will output the call tree:

```
│ RATIO │  TOTAL  │  SELF   │ TREE
│       │         │         │
│ 100.0 │ 12584   │ 0       │ dyld
│ 100.0 │ 12584   │ 8151    │ └─ dump_syms
│ 26.4  │ 3328    │ 2777    │    ├─ libsystem_malloc.dylib
│ 3.9   │ 487     │ 487     │    │  ├─ libsystem_platform.dylib
│ 0.5   │ 64      │ 64      │    │  └─ libsystem_kernel.dylib
│ 5.8   │ 731     │ 731     │    ├─ libsystem_platform.dylib
│ 3.0   │ 372     │ 372     │    ├─ libsystem_kernel.dylib
│ 0.0   │ 1       │ 1       │    ├─ libdyld.dylib
│ 0.0   │ 1       │ 0       │    └─ libsystem_c.dylib
│ 0.0   │ 1       │ 0       │       └─ libsystem_notify.dylib
│ 0.0   │ 1       │ 1       │          └─ libsystem_kernel.dylib
```

## Starknet

The crate also contains the `cairo-native` example, that uses the provided utilities to process samples from a transaction execution. See [STARKNET](STARKNET.md) for more information.
