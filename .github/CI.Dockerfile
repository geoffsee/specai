# CI Base Image for spec-ai
# Pre-installs all system dependencies needed to build the project.
#
# Build: docker build -t spec-ai-ci-base:latest -f vm/CI.Dockerfile .

FROM rust:latest

# Install system dependencies
RUN apt-get update && apt-get install -y \
    # Build essentials
    pkg-config \
    cmake \
    build-essential \
    # SSL/TLS
    libssl-dev \
    # Network tools
    curl \
    wget \
    # Archive tools
    unzip \
    # Audio (for spider/media dependencies)
    libasound2-dev \
    # OCR support (for extractous)
    tesseract-ocr \
    tesseract-ocr-eng \
    # Java (required for extractous/tika-native)
    default-jdk \
    # Clang/LLVM (for bindgen)
    libclang-dev \
    clang \
    && rm -rf /var/lib/apt/lists/*

# Set JAVA_HOME for extractous/tika builds
ENV JAVA_HOME=/usr/lib/jvm/default-java

WORKDIR /build

# Install DuckDB development libraries (must match libduckdb-sys version)
ARG DUCKDB_VERSION=1.4.1
RUN wget -q https://github.com/duckdb/duckdb/releases/download/v${DUCKDB_VERSION}/libduckdb-linux-amd64.zip \
    && unzip libduckdb-linux-amd64.zip -d /tmp/duckdb \
    && mv /tmp/duckdb/libduckdb.so /usr/local/lib/ \
    && mv /tmp/duckdb/duckdb.h /tmp/duckdb/duckdb.hpp /usr/local/include/ \
    && rm -rf /tmp/duckdb libduckdb-linux-amd64.zip \
    && echo "/usr/local/lib" > /etc/ld.so.conf.d/local.conf \
    && ldconfig

# Set library paths for linker
ENV LIBRARY_PATH=/usr/local/lib:$LIBRARY_PATH
ENV LD_LIBRARY_PATH=/usr/local/lib:$LD_LIBRARY_PATH

# Configure Cargo to find system DuckDB
RUN mkdir -p /root/.cargo && printf '[env]\nDUCKDB_LIB_DIR = "/usr/local/lib"\nDUCKDB_INCLUDE_DIR = "/usr/local/include"\n' > /root/.cargo/config.toml

RUN curl https://install.duckdb.org | sh

CMD ["rustc", "--version"]