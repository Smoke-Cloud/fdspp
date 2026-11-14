set windows-shell := ["powershell.exe", "-NoLogo", "-Command"]

alias b := build

_default:
    @just --list

# Run the tests
test:
    cargo test

# Build the debug binaries
build:
    cargo b

# Build release and create MSI package
package:
    cargo build --release
    mkdir -Force dist
    wix build FdsPpInstaller.wxs

# Clean the ./dist folder
clean-dist:
    rm -rf dist

# Clean everything
clean: clean-dist
    cargo clean

wasm:
    cargo build -p fdspp-wasm --target wasm32-unknown-unknown --profile wasm-release
    cp target/wasm32-unknown-unknown/wasm-release/fdspp_wasm.wasm wasm/pkg/fdspp.wasm

# Check the WASM package gives the same allocations as the command
test-wasm: wasm
    cargo build --release
    node wasm/test.mjs
