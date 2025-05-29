# Profiler SDK

This crate contains the type definitions and utilities for processing samples in the [processed profile format](https://github.com/firefox-devtools/profiler/blob/main/docs-developer/processed-profile-format.md). The crate is agnostic to the Starknet context.

In the future, it may be useful tu support other formats like [Gecko profile format](https://github.com/firefox-devtools/profiler/blob/main/docs-developer/gecko-profile-format.md), and [perf data format](https://git.kernel.org/pub/scm/linux/kernel/git/perf/perf-tools-next.git/tree/tools/perf/Documentation/perf.data-file-format.txt).

## Obtaining the Profile

First, you need a profile to process. You can generate one yourself by using `samply`:
```bash
samply record -- <command> <args>
```
You can also reuse the profile of someone else, like this one: https://share.firefox.dev/3H8dGXU.

Once the firefox profiler has loaded the profile, you can download it to a `json.gz` file, which can be uncompressed to the raw `.json` profile file.

## Starknet

The crate also contains the `cairo-native` example, that uses the provided utilities to process samples from a transaction execution. See [STARKNET](STARKNET.md) for more information.
