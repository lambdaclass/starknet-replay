#!/bin/sh
#
# It sets the LLVM environment variables.
#
# You can copy this file to .envrc/.env and adapt it for your environment.

case $(uname) in
  Darwin)
    # If installed with brew
    LLVM_SYS_201_PREFIX="$(brew --prefix llvm@20)"
    MLIR_SYS_200_PREFIX="$(brew --prefix llvm@20)"
    TABLEGEN_200_PREFIX="$(brew --prefix llvm@20)"

    export LLVM_SYS_201_PREFIX
    export MLIR_SYS_200_PREFIX
    export TABLEGEN_200_PREFIX
  ;;
  Linux)
    # If installed from Debian/Ubuntu repository:
    LIBRARY_PATH=/opt/homebrew/lib
    LLVM_SYS_201_PREFIX=/usr/lib/llvm-20
    MLIR_SYS_200_PREFIX=/usr/lib/llvm-20
    TABLEGEN_200_PREFIX=/usr/lib/llvm-20

    export LIBRARY_PATH
    export LLVM_SYS_201_PREFIX
    export MLIR_SYS_200_PREFIX
    export TABLEGEN_200_PREFIX
  ;;
esac

# export RPC_ENDPOINT_MAINNET=
# export RPC_ENDPOINT_TESTNET=

echo "loaded LLVM environment variables"
echo "remember you must manually set:"
echo "- RPC_ENDPOINT_MAINNET=rpc.endpoint.mainnet.com"
echo "- RPC_ENDPOINT_TESTNET=rpc.endpoint.testnet.com"
