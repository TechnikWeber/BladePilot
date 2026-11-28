#!/usr/bin/env bash
# Builds the Linux release archive of the command line from a compiled (static) binary. The
# desktop app's deb and rpm packages carry the command too; this archive is for machines
# without a desktop, such as a Raspberry Pi next to the plotter.
# Usage: VERSION=0.1.0 packaging/linux/package.sh <target-triple> <x86_64|aarch64>
set -euo pipefail
target=$1 arch=$2
version=${VERSION:?set VERSION}
bin=target/$target/release/bladepilot
out=dist/out
mkdir -p "$out"

# Plain archive
name=bladepilot-cli-$version-linux-$arch
mkdir -p "dist/$name"
cp "$bin" README.md LICENSE packaging/linux/70-bladepilot.rules "dist/$name/"
tar -C dist -czf "$out/$name.tar.gz" "$name"

ls -la "$out"
