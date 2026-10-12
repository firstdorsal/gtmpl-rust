#!/usr/bin/env bash
# Compiles and runs every ```rust block in the root README against this working tree.
#
# The engine's own README is doc-tested through `include_str!` from `lib.rs`, but the
# root README cannot be: `include_str!` may not reach outside the package directory, or
# the file would be missing from the published tarball. So it is checked here instead,
# and the `readme` job in CI is what makes the check binding.
#
# A block that defines `fn main` is used as it stands; anything else is wrapped in one,
# which is why a block may open with `use` statements -- those are legal inside a
# function body.
set -euo pipefail

repository="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
workspace="$(mktemp -d)"
trap 'rm -rf "$workspace"' EXIT

mkdir -p "$workspace/src/bin"

# Path dependencies, so this tests the code in the working tree rather than whatever is
# on crates.io. Every feature is on, because the README documents all of them.
cat > "$workspace/Cargo.toml" <<MANIFEST
[package]
name = "readme-examples"
version = "0.0.0"
edition = "2021"

[dependencies]
gtmpl-ng = { path = "$repository/gtmpl-ng", features = ["all-functions", "gtmpl_dynamic_template", "gtmpl_value_compat"] }
gtmpl_value_ng = { path = "$repository/gtmpl_value_ng" }
gtmpl_derive_ng = { path = "$repository/gtmpl_derive_ng" }
MANIFEST

python3 - "$repository/README.md" "$workspace/src/bin" <<'EXTRACT'
import pathlib
import re
import sys

readme, destination = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
blocks = re.findall(r"```rust\n(.*?)```", readme.read_text(), re.S)
if not blocks:
    sys.exit("no rust blocks found in the README -- the extraction is out of date")

for index, block in enumerate(blocks):
    source = block if re.search(r"^fn main\(", block, re.M) else f"fn main() {{\n{block}}}\n"
    (destination / f"block_{index}.rs").write_text(source)

print(f"extracted {len(blocks)} rust blocks")
EXTRACT

cd "$workspace"
for binary in src/bin/*.rs; do
    name="$(basename "$binary" .rs)"
    echo "--- running $name"
    cargo run --quiet --bin "$name"
done

echo "every README example compiled and ran"
