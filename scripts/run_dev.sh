#!/usr/bin/bash
HEADLINE="Running dev"

cd "$(dirname "$0")/.." || {
    echo "Failed to change to parent directory"
    exit 1
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
    echo "Running omni node with log"
    polkadot-omni-node --chain ./chain_spec.json --dev --prometheus-port 9615 --offchain-worker always --enable-offchain-indexing true --rpc-methods unsafe --log offchain=trace,logger=trace
    return $?
  else
    echo "Polkadot omni node crate is not installed"
    return 1
  fi
}
echo "$HEADLINE"
create_dev_chain_specs && run_omni_node
echo "Finish running"




