FROM rust:slim

# Install dependencies
RUN apt-get update && \
    apt-get install -y make curl git libsqlite3-dev fontconfig libfontconfig1-dev libfontconfig libssl-dev libclang-dev


# Copy in project files
COPY . /app
WORKDIR /app

# Change to a non-root user
RUN useradd -ms /bin/bash appuser && \
    chown -R appuser:appuser /app
USER appuser

# Build & install contender cli from source
RUN cargo install --path ./crates/cli

# Import test fixtures into .contender
RUN mkdir -p /home/appuser/.contender && \
    cp /app/test_fixtures/* /home/appuser/.contender

# prevent contender from trying to open a browser
ENV BROWSER=none
# use cached test data for reports
ENV DEBUG_USEFILE=true

ENTRYPOINT ["contender"]
CMD ["--help"]

# to override test data or persist results, mount host directory to:
#   /home/appuser/.contender[/reports]
