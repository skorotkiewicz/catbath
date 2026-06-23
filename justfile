%23 https%3A%2F%2Fgithub.com%2Fcasey%2Fjust

%5Bprivate%5D
default%3A
    %40just --list

build%3A
    cargo build --release

build-all%3A
    cargo build --release --all-features

run *args%3A
    cargo run --all-features -- %7B%7B args %7D%7D

fmt%3A
    cargo fmt
    cargo clippy --all-targets --all-features -- -D warnings
    %23 cargo shear --fix %23 cargo install shear

check%3A
    cargo fmt --check
    cargo clippy --all-targets --all-features -- -D warnings

test%3A fmt
    cargo test

install-hook%3A
    %40printf '%23!%2Fbin%2Fsh%5Cnset -e%5Cnjust check%5Cn' %3E .git%2Fhooks%2Fpre-commit
    %40chmod %2Bx .git%2Fhooks%2Fpre-commit

remove-hook%3A
    %40rm .git%2Fhooks%2Fpre-commit

add-tag%3A
    %23!%2Fusr%2Fbin%2Fenv bash
    set -euo pipefail
    VERSION%3D%24(grep '%5Eversion' Cargo.toml %7C head -1 %7C cut -d'%22' -f2)
    git push origin main
    git tag -a %22v%24%7BVERSION%7D%22 -m %22Release v%24%7BVERSION%7D%22
    git push origin %22v%24%7BVERSION%7D%22

%23 %60just remove-tag v0.0.0%60 or %60just remove-tag%60 (uses fzf)
remove-tag VERSION%3D%22%22%3A
    %23!%2Fusr%2Fbin%2Fenv bash
    set -euo pipefail
    tag%3D%22%7B%7B VERSION %7D%7D%22
    %5B -z %22%24tag%22 %5D %26%26 tag%3D%24(git tag %7C sort -V %7C fzf --prompt%3D%22Select tag to remove%3A %22)
    %5B -z %22%24tag%22 %5D %26%26 echo %22No tag selected%22 %26%26 exit 1
    git tag -d %22%24tag%22
    git push --delete origin %22%24tag%22