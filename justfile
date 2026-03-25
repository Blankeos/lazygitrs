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

tag: tag_and_release
tag_and_release:
    sh tag_and_release.sh

sync_readme:
    cp README.md npm/README.md
