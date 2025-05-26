# just manual: https://github.com/casey/just

serve *args:
    #!/bin/sh
    export RUST_BACKTRACE=0
    cargo build
    ./target/debug/home cub {{args}}

serve-release *args:
    #!/bin/sh
    export RUST_BACKTRACE=0
    cargo build --release
    ./target/release/home cub {{args}}

repack:
    beardist build
    ./repack.sh
