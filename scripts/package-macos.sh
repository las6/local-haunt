#!/bin/bash
set -euo pipefail

project_root="$(cd "$(dirname "$0")/.." && pwd)"
cd "$project_root"
profile=release
run_app=false
for argument in "$@"; do
    case "$argument" in
        --debug) profile=debug ;;
        --run) run_app=true ;;
        *) echo "Usage: $0 [--debug] [--run]" >&2; exit 2 ;;
    esac
done

if [[ "$profile" == release ]]; then
    cargo build --locked --release
else
    cargo build --locked
fi

app_bundle="$project_root/target/app/$profile/Local Haunt.app"
mkdir -p "$app_bundle/Contents/MacOS" "$app_bundle/Contents/Resources"
cp "target/$profile/local-haunt" "$app_bundle/Contents/MacOS/local-haunt"
cp assets/local-haunt.icns "$app_bundle/Contents/Resources/local-haunt.icns"
cp assets/Info.plist "$app_bundle/Contents/Info.plist"
# Local ad-hoc signing needs no paid developer account.
codesign --force --sign - "$app_bundle"
printf 'Built: %s\n' "$app_bundle"

if [[ "$run_app" == true ]]; then
    # Keep the app attached to Zed's task terminal so stopping the task stops it.
    exec "$app_bundle/Contents/MacOS/local-haunt"
fi
