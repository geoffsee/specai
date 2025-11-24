# EXAMPLE: The built image is untenably massive. It needs multi-stage build to extract a minimal runtime image.
FROM rust:latest AS builder

RUN apt-get update && apt-get install -y \
    pkg-config \
    cmake \
    libssl-dev \
    curl \
    wget \
    unzip \
    libasound2-dev \
    tesseract-ocr \
    tesseract-ocr-eng \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /build

COPY . .

ENTRYPOINT ["cargo"]
CMD ["test"]