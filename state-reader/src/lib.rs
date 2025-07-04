//! This crate contains logic for reading a starknet node's state
//!
//! The main structure is [FullStateReader](`full_state_reader::FullStateReader`),
//! which contains logic for:
//! - Reading data from starknet node through RPC calls.
//! - Caching data both in memory and on disk.
//! - Compiling contracts into both CASM and Native.
//!
//! A fully cached reader will show no additional overhead, and is safe to
//! use for benchmarking. To ensure that the reader is fully cached, the
//! transaction can be executed twice.
//!
//! The cache is saved to the relative directory `./cache/`:
//! - `./cache/rpc.json`: Contains raw rpc data.
//! - `./cache/native/`: Contains compiled Native classes.
//! - `./cache/casm/`: Contains compiled CASM classes.
//!
//! The wrapper [BlockStateReader](`block_state_reader::BlockStateReader`) calls
//! the full reader, but for a particular block. Its used for executing a particular
//! transaction.

pub mod block_state_reader;
pub mod class_manager;
pub mod full_state_reader;
pub mod objects;
pub mod remote_state_reader;
pub mod state_cache;
