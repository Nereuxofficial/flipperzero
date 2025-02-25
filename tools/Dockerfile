FROM --platform=x86_64 debian:bookworm AS firmware-builder

# FIRMWARE_GIT should be a git repo with the firmware source code
ARG FIRMWARE_GIT=https://github.com/flipperdevices/flipperzero-firmware.git
ARG BRANCH=0.88.0

# Install dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    ca-certificates \
    git \
    curl \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

RUN git clone -b $BRANCH --recursive $FIRMWARE_GIT firmware

WORKDIR /app/firmware

RUN ./fbt

####################
# bindgen
FROM --platform=x86_64 rust:bookworm AS builder

# Install dependencies
RUN apt-get update && apt-get install -y \
    build-essential \
    ca-certificates \
    libudev-dev \
    && rm -rf /var/lib/apt/lists/*

RUN rustup component add rustfmt

WORKDIR /app

RUN curl -L https://github.com/llvm/llvm-project/releases/download/llvmorg-16.0.4/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04.tar.xz | tar -x --xz

COPY --from=firmware-builder /app/firmware /app/firmware

WORKDIR /app/flipperzero-rs/

COPY . .

WORKDIR /app/flipperzero-rs/tools

# We don't use debian's libclang. For details see https://github.com/flipperzero-rs/flipperzero/pull/70#discussion_r1199723419
RUN env LIBCLANG_PATH=/app/clang+llvm-16.0.4-x86_64-linux-gnu-ubuntu-22.04/lib cargo run --bin generate-bindings /app/firmware/build/f7-firmware-D/sdk_headers/

RUN cp bindings.rs ../crates/sys/src

WORKDIR /app/flipperzero-rs/crates

# test it builds
RUN cargo build --release

####################
ENTRYPOINT [ "cat", "sys/src/bindings.rs" ]
