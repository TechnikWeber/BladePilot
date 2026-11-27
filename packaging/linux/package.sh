#!/usr/bin/env bash
# Builds the Linux release files from a compiled binary: .tar.gz, .deb, .rpm and AppImage.
# Needs nfpm and appimagetool on PATH (the release workflow downloads both).
# Usage: VERSION=0.1.0 packaging/linux/package.sh <target-triple> <x86_64|aarch64> <amd64|arm64>
set -euo pipefail
target=$1 arch=$2 nfpm_arch=$3
version=${VERSION:?set VERSION}
bin=target/$target/release/bladepilot
out=dist/out
mkdir -p "$out" dist/stage
cp "$bin" dist/stage/bladepilot

# Plain archive
name=BladePilot-$version-linux-$arch
mkdir -p "dist/$name"
cp "$bin" README.md LICENSE packaging/linux/70-bladepilot.rules "dist/$name/"
tar -C dist -czf "$out/$name.tar.gz" "$name"

# Debian/Ubuntu and Fedora/openSUSE packages
for packager in deb rpm; do
  VERSION=$version NFPM_ARCH=$nfpm_arch nfpm package --config packaging/linux/nfpm.yaml --packager "$packager" --target "$out/"
done

# AppImage: one file, runs on any distribution
appdir=dist/BladePilot.AppDir
mkdir -p "$appdir/usr/bin"
cp "$bin" "$appdir/usr/bin/bladepilot"
cp packaging/linux/bladepilot.desktop "$appdir/bladepilot.desktop"
cp packaging/icons/bladepilot.png "$appdir/bladepilot.png"
ln -sf usr/bin/bladepilot "$appdir/AppRun"
update="gh-releases-zsync|TechnikWeber|BladePilot|latest|BladePilot-*-$arch.AppImage.zsync"
(cd "$out" && ARCH=$arch APPIMAGE_EXTRACT_AND_RUN=1 appimagetool -u "$update" "../../$appdir" "BladePilot-$version-$arch.AppImage")
ls -la "$out"
