# Profiler SDK

This crate contains the type definitions and utilities for processing samples in the [processed profile format](https://github.com/firefox-devtools/profiler/blob/main/docs-developer/processed-profile-format.md).

In the future, it may be useful tu support other formats like [Gecko profile format](https://github.com/firefox-devtools/profiler/blob/main/docs-developer/gecko-profile-format.md), and [perf data format](https://git.kernel.org/pub/scm/linux/kernel/git/perf/perf-tools-next.git/tree/tools/perf/Documentation/perf.data-file-format.txt).

## Cairo Native

The crate also contains the `cairo-native` example, that uses the provided utilities to process samples from a transaction execution. See [STARKNET](STARKNET.md) for more information.
