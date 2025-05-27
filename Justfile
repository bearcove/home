# just manual: https://github.com/casey/just

serve *args:
    #!/bin/sh
    export RUST_BACKTRACE=0
    cargo build
    ./target/debug/home serve {{args}}

serve-release *args:
    #!/bin/sh
    export RUST_BACKTRACE=0
    cargo build --release
    ./target/release/home serve {{args}}

install:
    #!/bin/sh
    cargo build --release
    mkdir -p ~/.cargo/bin
    cp ./target/release/home ~/.cargo/bin/
    cp ./target/release/home-* ~/.cargo/bin/ 2>/dev/null || true


repack:
    beardist build
    ./repack.sh
