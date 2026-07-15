#!/bin/sh

set -eu

default_identity='Apple Development: Jeph Liu (7CJV28MFNU)'
identifier='io.jeph.venmo'

if [ "$#" -lt 1 ]; then
    printf '%s\n' 'error: the Cargo runner requires an executable path' >&2
    exit 2
fi

executable=$1
shift

# Cargo uses this runner for every executable target on macOS. Only the actual
# CLI has a Keychain identity that must remain stable; test and benchmark
# harnesses should retain Cargo's normal signatures.
if [ "${executable##*/}" != 'venmo' ]; then
    exec "$executable" "$@"
fi

if [ "${VENMO_CODESIGN_IDENTITY+x}" = x ]; then
    if [ -z "$VENMO_CODESIGN_IDENTITY" ]; then
        printf '%s\n' 'error: VENMO_CODESIGN_IDENTITY is set but empty' >&2
        exit 2
    fi

    identity=$VENMO_CODESIGN_IDENTITY
    identity_is_required=true
else
    identity=$default_identity
    identity_is_required=false
fi

available_identities=$(/usr/bin/security find-identity -v -p codesigning 2>/dev/null || true)
if ! printf '%s\n' "$available_identities" | /usr/bin/grep -Fq -- "$identity"; then
    if [ "$identity_is_required" = true ]; then
        printf 'error: requested macOS code-signing identity is unavailable: %s\n' "$identity" >&2
        exit 1
    fi

    printf 'warning: macOS code-signing identity is unavailable; running without development signature: %s\n' "$identity" >&2
    exec "$executable" "$@"
fi

/usr/bin/codesign \
    --force \
    --sign "$identity" \
    --identifier "$identifier" \
    --timestamp=none \
    "$executable"

/usr/bin/codesign --verify --strict --verbose=2 "$executable"

exec "$executable" "$@"
