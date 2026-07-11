#!/bin/sh
set -eu

ROOT=$(CDPATH= cd -- "$(dirname "$0")/../.." && pwd)
LAUNCHER="$ROOT/scripts/run-mac-client.sh"

if [ ! -x "$LAUNCHER" ]; then
  echo "FAIL: launcher missing or not executable" >&2
  exit 1
fi

help=$($LAUNCHER --help)
printf '%s' "$help" | grep -q -- '--with-server'

if "$LAUNCHER" --unknown >/dev/null 2>&1; then
  echo "FAIL: unknown option succeeded" >&2
  exit 1
fi

tmp=$(mktemp -d)
trap 'rm -rf "$tmp"' EXIT
log="$tmp/calls"
mkdir -p "$tmp/bin" "$tmp/repo/scripts" "$tmp/repo/clients/apple/Packages/CoreFFI/scripts" \
  "$tmp/repo/clients/apple/Packages/CoreFFI/MusicCoreFFI.xcframework" "$tmp/repo/core/src"
cp "$LAUNCHER" "$tmp/repo/scripts/run-mac-client.sh"
printf 'input' > "$tmp/repo/core/src/lib.rs"
printf 'output' > "$tmp/repo/clients/apple/Packages/CoreFFI/MusicCoreFFI.xcframework/Info.plist"
touch -t 202601010000 "$tmp/repo/core/src/lib.rs"
touch -t 202601020000 "$tmp/repo/clients/apple/Packages/CoreFFI/MusicCoreFFI.xcframework/Info.plist"

for command in cargo swift docker; do
  cat > "$tmp/bin/$command" <<EOF
#!/bin/sh
echo "$command \$*" >> "$log"
EOF
  chmod +x "$tmp/bin/$command"
done
cat > "$tmp/bin/uname" <<'EOF'
#!/bin/sh
echo Darwin
EOF
chmod +x "$tmp/bin/uname"
cat > "$tmp/repo/clients/apple/Packages/CoreFFI/scripts/build-core.sh" <<EOF
#!/bin/sh
echo "build-core" >> "$log"
EOF
chmod +x "$tmp/repo/clients/apple/Packages/CoreFFI/scripts/build-core.sh"

PATH="$tmp/bin:$PATH" "$tmp/repo/scripts/run-mac-client.sh"
grep -q '^swift run --package-path clients/apple MusicApp$' "$log"
if grep -q '^build-core$' "$log"; then
  echo "FAIL: fresh bindings rebuilt" >&2
  exit 1
fi

: > "$log"
touch "$tmp/repo/core/src/lib.rs"
PATH="$tmp/bin:$PATH" "$tmp/repo/scripts/run-mac-client.sh" --with-server
grep -q '^docker compose up -d$' "$log"
grep -q '^build-core$' "$log"
grep -q '^swift run --package-path clients/apple MusicApp$' "$log"

echo "run-mac-client tests: PASS"
