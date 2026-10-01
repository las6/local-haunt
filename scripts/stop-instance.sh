#!/bin/bash
set -euo pipefail

# Match the executable path, never the app name or command-line arguments.
app_executable=${1:?Usage: stop-instance.sh /absolute/path/to/executable}
if [[ "$app_executable" != /* ]]; then
    echo "The app executable must use an absolute path." >&2
    exit 2
fi

processes=$(/bin/ps -axo pid=,comm=)
pids=$(printf '%s\n' "$processes" | LOCAL_HAUNT_EXECUTABLE="$app_executable" /usr/bin/awk '
    {
        pid = $1
        sub(/^[[:space:]]*[0-9]+[[:space:]]+/, "")
        if ($0 == ENVIRON["LOCAL_HAUNT_EXECUTABLE"]) print pid
    }
')

for pid in $pids; do
    # Recheck before signalling in case the process exited since discovery.
    command=$(/bin/ps -p "$pid" -o comm=) || continue
    [[ "$command" == "$app_executable" ]] || continue
    identity=$(/bin/ps -p "$pid" -o lstart=,comm=) || continue
    [[ "$identity" == *" $app_executable" ]] || continue
    current=$(/bin/ps -p "$pid" -o lstart=,comm=) || continue
    [[ "$current" == "$identity" ]] || continue
    printf 'Stopping previous app instance (PID %s).\n' "$pid"
    if ! kill -TERM "$pid"; then
        # An exit between the identity check and kill is harmless.
        if kill -0 "$pid" 2>/dev/null; then
            echo "Could not stop the previous app instance." >&2
            exit 1
        fi
    fi
    for ((attempt = 0; attempt < 50; attempt++)); do
        current=$(/bin/ps -p "$pid" -o lstart=,comm=) || break
        [[ "$current" == "$identity" ]] || break
        sleep 0.1
    done
    current=$(/bin/ps -p "$pid" -o lstart=,comm=) || current=""
    if [[ "$current" == "$identity" ]]; then
        echo "The previous app instance did not exit. Quit it, then rerun the task." >&2
        exit 1
    fi
done
