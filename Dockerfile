####################################################################################################
FROM ghcr.io/bearcove/beardist AS base

COPY rust-toolchain.toml .
RUN cargo binstall -y cargo-chef sccache
ENV RUSTC_WRAPPER=sccache SCCACHE_DIR=/sccache

FROM base AS planner
WORKDIR /app
COPY . .
RUN cargo chef prepare --recipe-path recipe.json

FROM base AS builder
WORKDIR /app
COPY --from=planner /app/recipe.json recipe.json
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo chef cook --release --recipe-path recipe.json
COPY . .
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/usr/local/cargo/git \
    --mount=type=cache,target=$SCCACHE_DIR,sharing=locked \
    cargo build --release && mkdir -p /app && cp target/release/home /app/

####################################################################################################
FROM ghcr.io/bearcove/base AS home

RUN set -eux; \
    export DEBIAN_FRONTEND=noninteractive \
    && apt-get update \
    && apt-get install --no-install-recommends -y \
    imagemagick \
    iproute2 \
    iputils-ping \
    dnsutils \
    curl \
    && rm -rf /var/lib/apt/lists/*
RUN set -eux; \
    echo "Checking for required tools..." && \
    which curl || (echo "curl not found" && exit 1) && \
    which tar || (echo "tar not found" && exit 1) && \
    which ip || (echo "ip not found" && exit 1) && \
    which ping || (echo "ping not found" && exit 1) && \
    which dig || (echo "dig not found" && exit 1) && \
    which nslookup || (echo "nslookup not found" && exit 1) && \
    echo "Creating FFmpeg directory..." && \
    mkdir -p /opt/ffmpeg && \
    echo "Downloading FFmpeg..." && \
    arch=$([ "$(uname -m)" = "aarch64" ] && echo "linuxarm64" || echo "linux64") && \
    echo "Downloading $arch build" && \
    curl -sSL --retry 3 --retry-delay 3 \
    "https://github.com/BtbN/FFmpeg-Builds/releases/download/latest/ffmpeg-master-latest-${arch}-gpl-shared.tar.xz" -o /tmp/ffmpeg.tar.xz && \
    echo "Extracting FFmpeg..." && \
    tar -xJf /tmp/ffmpeg.tar.xz --strip-components=1 -C /opt/ffmpeg && \
    rm -f /tmp/ffmpeg.tar.xz
ENV \
    FFMPEG=/opt/ffmpeg \
    PATH=$PATH:/opt/ffmpeg/bin \
    LD_LIBRARY_PATH=/opt/ffmpeg/lib
RUN set -eux; \
    echo "Verifying FFmpeg installation..." && \
    ffmpeg -version || (echo "FFmpeg installation failed" && exit 1) && \
    echo "FFmpeg installation successful"

# apparently `libsqlite3.so` is only installed by the `-dev` package, but our program relies on it, so...
RUN arch=$([ "$(uname -m)" = "aarch64" ] && echo "aarch64" || echo "x86_64") \
    && ln -s "/usr/lib/${arch}-linux-gnu/libsqlite3.so.0" "/usr/lib/${arch}-linux-gnu/libsqlite3.so"

RUN set -eux; \
    echo "Installing uv (Python package manager)..." && \
    curl -sSL --retry 3 --retry-delay 3 https://astral.sh/uv/install.sh | sh

RUN set -eux; \
    echo "Installing home-drawio..." && \
    homedrawio_version="v1.0.3" && \
    arch=$([ "$(uname -m)" = "aarch64" ] && echo "aarch64-unknown-linux-gnu" || echo "x86_64-unknown-linux-gnu") && \
    curl -sSL --retry 3 --retry-delay 3 \
    "https://github.com/bearcove/home-drawio/releases/download/${homedrawio_version}/${arch}.tar.xz" -o /tmp/home-drawio.tar.xz && \
    tar -xJf /tmp/home-drawio.tar.xz -C /usr/local/bin && \
    chmod +x /usr/local/bin/home-drawio && \
    rm -f /tmp/home-drawio.tar.xz

COPY --from=builder /app/home /usr/bin/home

####################################################################################################
FROM scratch AS home-minimal

COPY --from=builder /app/home /home
