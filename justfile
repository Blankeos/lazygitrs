default:
    just --list

ref-pull:
    bun scripts/fetch-references.ts pull

ref-clone:
    bun scripts/fetch-references.ts clone

preview:
    ./target/debug/lazygitrs

# as
rpreview:
    ./target/release/lazygitrs

# Release: bump versions, create a release commit, and push a git tag.
# Must be run from an up-to-date main — the script will refuse other branches.
tag: tag_and_release
tag_and_release:
    sh tag_and_release.sh

sync_readme:
    cp README.md npm/README.md

gen-benchmarks:
    bun scripts/gen-benchmarks.ts
    just sync_readme
