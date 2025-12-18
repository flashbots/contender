# --- Builder stage ---
FROM rust:slim AS builder

# Install build dependencies
RUN apt-get update && \
    apt-get install -y make curl git libsqlite3-dev fontconfig libfontconfig1-dev libfontconfig libssl-dev libclang-dev uuid-dev && \
    rm -rf /var/lib/apt/lists/*

# Copy in project files
COPY . /app
WORKDIR /app

# Build contender cli from source
RUN cargo install --locked --path ./crates/cli --root /app/contender-dist

# Install anvil (foundry)
RUN curl -L https://foundry.paradigm.xyz | bash && \
    /root/.foundry/bin/foundryup && \
    cp /root/.foundry/bin/anvil /app/contender-dist/bin/

# --- Runtime stage ---
FROM debian:trixie-slim

# Install runtime dependencies
RUN apt-get update && \
    apt-get install -y libsqlite3-0 fontconfig libfontconfig1 libssl3 clang && \
    rm -rf /var/lib/apt/lists/*

# Copy built binary and test fixtures from builder
COPY --from=builder /app/contender-dist /root/.cargo

# Set permissions
RUN mkdir -p /root/.contender

# prevent contender from trying to open a browser
ENV BROWSER=none

ENV PATH="/root/.cargo/bin:${PATH}"

# to override test data or persist results, mount host directory to:
#   /root/.contender[/reports]

ENTRYPOINT ["contender"]
CMD ["--help"]
