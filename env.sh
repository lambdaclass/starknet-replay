#!/bin/sh
#
# It sets the LLVM environment variables.
#
# You can copy this file to .envrc/.env and adapt it for your environment.

case $(uname) in
  Darwin)
    # If installed with brew
    LLVM_SYS_191_PREFIX="$(brew --prefix llvm@19)"
    MLIR_SYS_190_PREFIX="$(brew --prefix llvm@19)"
    TABLEGEN_190_PREFIX="$(brew --prefix llvm@19)"

    export LLVM_SYS_191_PREFIX
    export MLIR_SYS_190_PREFIX
    export TABLEGEN_190_PREFIX
  ;;
  Linux)
    # If installed from Debian/Ubuntu repository:
    LIBRARY_PATH=/opt/homebrew/lib
    LLVM_SYS_191_PREFIX=/usr/lib/llvm-19
    MLIR_SYS_190_PREFIX=/usr/lib/llvm-19
    TABLEGEN_190_PREFIX=/usr/lib/llvm-19

    export LIBRARY_PATH
    export LLVM_SYS_191_PREFIX
    export MLIR_SYS_190_PREFIX
    export TABLEGEN_190_PREFIX
  ;;
esac

# export RPC_ENDPOINT_MAINNET=
# export RPC_ENDPOINT_TESTNET=

echo "loaded LLVM environment variables"
echo "remember you must manually set:"
echo "- RPC_ENDPOINT_MAINNET=rpc.endpoint.mainnet.com"
echo "- RPC_ENDPOINT_TESTNET=rpc.endpoint.testnet.com"
