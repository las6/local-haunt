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
# Use the compiled theme and the same SVG renderer as GPUI for every icon size.
iconset="$project_root/target/app/$profile/local-haunt.iconset"
"target/$profile/local-haunt" --export-iconset "$iconset"
/usr/bin/iconutil --convert icns --output "$app_bundle/Contents/Resources/local-haunt.icns" "$iconset"
cp assets/Info.plist "$app_bundle/Contents/Info.plist"
cp assets/Credits.html "$app_bundle/Contents/Resources/Credits.html"
# Cargo is the source of truth for the version shown by the native About panel.
app_version=$("target/$profile/local-haunt" --version)
/usr/libexec/PlistBuddy -c "Set :CFBundleShortVersionString $app_version" "$app_bundle/Contents/Info.plist"
/usr/libexec/PlistBuddy -c "Set :CFBundleVersion $app_version" "$app_bundle/Contents/Info.plist"
# Local ad-hoc signing needs no paid developer account.
codesign --force --sign - "$app_bundle"
printf 'Built: %s\n' "$app_bundle"

if [[ "$run_app" == true ]]; then
    # Replace only instances launched from this exact project/profile bundle.
    bash "$project_root/scripts/stop-instance.sh" "$app_bundle/Contents/MacOS/local-haunt"
    # Keep the app attached to Zed's task terminal so stopping the task stops it.
    exec "$app_bundle/Contents/MacOS/local-haunt"
fi
