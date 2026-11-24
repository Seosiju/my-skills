#!/bin/sh
set -eu

fail() {
    printf 'my-skills install: %s\n' "$1" >&2
    exit 1
}

platform=$(uname -s)
architecture=$(uname -m)
case "$platform:$architecture" in
    Darwin:arm64|Darwin:aarch64) target=aarch64-apple-darwin ;;
    Darwin:x86_64) target=x86_64-apple-darwin ;;
    Linux:x86_64) target=x86_64-unknown-linux-gnu ;;
    *) fail "unsupported platform: $platform $architecture" ;;
esac
archive=my-skills-$target.tar.gz

releases=https://github.com/Seosiju/my-skills/releases
version=${MY_SKILLS_VERSION:-}
case "$version" in
    *[!a-zA-Z0-9.-]*|.|..) fail 'MY_SKILLS_VERSION must be a release tag' ;;
esac
if [ -n "${MY_SKILLS_RELEASE_BASE_URL:-}" ]; then
    base_url=${MY_SKILLS_RELEASE_BASE_URL%/}
elif [ -n "$version" ]; then
    base_url=$releases/download/$version
else
    base_url=$releases/latest/download
fi

install_dir=${MY_SKILLS_INSTALL_DIR:-"$HOME/.local/bin"}
case "$install_dir" in
    /*) ;;
    *) install_dir=$PWD/$install_dir ;;
esac

work_dir=$(mktemp -d "${TMPDIR:-/tmp}/my-skills.XXXXXXXX")
staged=
cleanup() {
    if [ -n "$staged" ]; then
        rm -f "$staged"
    fi
    rm -rf "$work_dir"
}
trap cleanup 0
trap 'exit 1' HUP INT TERM

curl --fail --silent --show-error --location --globoff \
    --output "$work_dir/SHA256SUMS" -- "$base_url/SHA256SUMS"
expected=$(awk -v name="$archive" '
    { sub(/\r$/, "") }
    $2 == name || $2 == "*" name {
        if (length($1) != 64 || $1 !~ /^[[:xdigit:]]+$/) exit 1
        checksum = tolower($1)
        matches++
    }
    END {
        if (matches != 1) exit 1
        print checksum
    }
' "$work_dir/SHA256SUMS") || fail "missing or invalid checksum for $archive"

curl --fail --silent --show-error --location --globoff \
    --output "$work_dir/$archive" -- "$base_url/$archive"
if command -v sha256sum >/dev/null 2>&1; then
    actual=$(sha256sum < "$work_dir/$archive")
else
    actual=$(shasum -a 256 < "$work_dir/$archive")
fi
[ "${actual%% *}" = "$expected" ] || fail "SHA-256 mismatch for $archive"

tar -xOzf "$work_dir/$archive" my-skills > "$work_dir/my-skills"
[ -s "$work_dir/my-skills" ] || fail "missing binary in $archive"

mkdir -p "$install_dir"
destination=$install_dir/my-skills
[ ! -d "$destination" ] || fail "expected a file at $destination"
staged=$(mktemp "$install_dir/.my-skills.XXXXXXXX")
install -m 0755 "$work_dir/my-skills" "$staged"
mv -f "$staged" "$destination"
staged=

printf 'Installed %s\n' "$destination"
case ":${PATH:-}:" in
    *":$install_dir:"*) ;;
    *) printf 'Add %s to PATH to run my-skills from any directory.\n' "$install_dir" ;;
esac
