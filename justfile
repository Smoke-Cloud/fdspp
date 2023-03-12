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
