set windows-shell := ["pwsh.exe", "-NoLogo", "-Command"]

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
    candle FdsPpInstaller.wxs
    light FdsPpInstaller.wixobj

# Clean the ./dist folder
clean-dist:
    rm -rf dist

# Clean everything
clean: clean-dist
    cargo clean
