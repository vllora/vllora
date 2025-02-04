# Use debian:bullseye-slim as base image since it has glibc 2.31
FROM --platform=linux/amd64 debian:bullseye-slim

# Install required packages
RUN apt-get update && apt-get install -y \
    curl \
    libssl-dev \
    && rm -rf /var/lib/apt/lists/*

# Create a non-root user to run the application
RUN groupadd -r aigateway && useradd -r -g aigateway aigateway

# Create directory for the application
WORKDIR /app

# Copy the binary from the build output
COPY target/x86_64-unknown-linux-gnu/release/ai-gateway /app/

# Set ownership of the application files
RUN chown -R aigateway:aigateway /app

# Switch to non-root user
USER aigateway

# Expose the port (assuming default HTTP port 8080 - adjust if needed)
EXPOSE 8080

# Run the binary
ENTRYPOINT ["/app/ai-gateway"] 