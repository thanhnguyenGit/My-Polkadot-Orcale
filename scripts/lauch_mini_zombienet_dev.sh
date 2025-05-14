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

function run_zombienet() {
  if command -v zombienet >/dev/null 2>&1; then
    echo "Running zombienet (native)"
    zombienet -p native spawn zombienet-configuration-files/spawn-a-basic-network.toml
    return $?
  else
    echo "Zombienet is not installed"
    return 1
  fi
}

function cleanup_zombienet() {
    echo "Cleaning up Zombienet state..."
    NODE_DATA_PATH="$HOME/.local/share/parachain-template-node"
    if [ -d "$NODE_DATA_PATH" ]; then
          echo "Cleaning up local node data at $NODE_DATA_PATH..."
          rm -rf "$NODE_DATA_PATH"
          echo "Local node data removed."
    else
          echo "No local node data found at $NODE_DATA_PATH."
    fi
    echo "Zombienet data removed from ~/.zombienet"
}

echo "$HEADLINE"
run_build_release && create_dev_chain_specs && run_zombienet && cleanup_zombienet
echo "Finish running"




