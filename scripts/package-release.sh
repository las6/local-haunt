#!/bin/bash
set -euo pipefail

project_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_root"
if [[ "$(uname -m)" != arm64 ]]; then
    echo "This download is Apple Silicon only. Build it on an arm64 Mac." >&2
    exit 1
fi
bash scripts/package-macos.sh
app_bundle="$project_root/target/app/release/Local Haunt.app"
app_executable="$app_bundle/Contents/MacOS/local-haunt"
if [[ "$(lipo -archs "$app_executable")" != arm64 ]]; then
    echo "The app executable must contain only the arm64 architecture." >&2
    exit 1
fi
codesign --verify --strict "$app_bundle"
app_version=$("$app_executable" --version)
archive_name="Local-Haunt-$app_version-macos-arm64.zip"
archive_folder="$project_root/target/releases"
mkdir -p "$archive_folder"
ditto -c -k --sequesterRsrc --keepParent "$app_bundle" "$archive_folder/$archive_name"
cd "$archive_folder"
shasum -a 256 "$archive_name" > "$archive_name.sha256"
printf 'Release download: %s\n' "$archive_folder/$archive_name"
