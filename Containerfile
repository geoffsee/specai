
FROM rust:latest AS builder
RUN apt-get update && apt-get install -y \
    pkg-config \
    cmake \
    libssl-dev \
    curl \
    wget \
    unzip \
    libasound2-dev \
    && rm -rf /var/lib/apt/lists/*
WORKDIR /build

COPY . .

RUN cargo build --release

ENTRYPOINT ["/build/target/release/spec-ai"]