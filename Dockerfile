####################################################################################################
FROM ghcr.io/bearcove/base AS home-base

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
