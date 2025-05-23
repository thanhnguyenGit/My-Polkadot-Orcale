#!/usr/bin/bash
HEADLINE="Running dev, run this test solo node logic only."

cd "$(dirname "$0")/.." || {
    echo "Failed to change to parent directory"
    exit 1
}

function run_build_release() {
    if command -v cargo build > /dev/null 2>&1; then
      echo "Buiding binary"
      cargo build --release
      return $?
    else
      echo "Cargo is not install, make sure to install lastest cargo"
      return 1
    fi
}

function create_dev_chain_specs() {
    if command -v chain-spec-builder >/dev/null 2>&1; then
      echo "Generating chain specs for dev"
      chain-spec-builder create -t development \
      --relay-chain rococo \
      --para-id 1000 \
      --runtime ./target/release/wbuild/parachain-template-runtime/parachain_template_runtime.compact.compressed.wasm \
      named-preset development
      return $?
    else
      echo "Chain-spec-builder crate is not installed"
      return 1
    fi
}

function run_omni_node() {
  if command -v polkadot-omni-node >/dev/null 2>&1; then
    echo "Running omni node"
    echo "WARNING: this network enable: Log, OCW, unsafe rpc-methods"
    polkadot-omni-node --chain ./chain_spec.json --collator --dev --prometheus-port 9615 --offchain-worker always --enable-offchain-indexing true --rpc-methods unsafe --log offchain=trace,logger=trace
    return $?
  else
    echo "Polkadot omni node crate is not installed"
    return 1
  fi
}
echo "$HEADLINE"
run_build_release && create_dev_chain_specs && run_omni_node
echo "Finish running"




