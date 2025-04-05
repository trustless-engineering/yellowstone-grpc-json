FROM --platform=$BUILDPLATFORM rust:1.86.0 AS builder

# Add only the target we need based on TARGETPLATFORM
ARG TARGETPLATFORM
RUN case "$TARGETPLATFORM" in \
    "linux/amd64") rustup target add x86_64-unknown-linux-musl ;; \
    "linux/arm64") rustup target add aarch64-unknown-linux-musl ;; \
    *) exit 1 ;; \
    esac

RUN apt update && apt install -y musl-dev musl-tools

ENV USER=fluvio
RUN useradd --create-home "$USER"
USER $USER
WORKDIR /home/fluvio

# Install FVM
RUN curl -fsS https://hub.infinyon.cloud/install/install.sh | bash
ENV PATH="/home/fluvio/.fvm/bin:${PATH}"
ENV PATH="/home/fluvio/.fluvio/bin:${PATH}"

RUN fvm install sdf-beta4

COPY . .

# Build only for the target architecture
ARG TARGETPLATFORM
RUN case "$TARGETPLATFORM" in \
    "linux/amd64") cdk build --target x86_64-unknown-linux-musl ;; \
    "linux/arm64") cdk build --target aarch64-unknown-linux-musl ;; \
    esac

RUN case "$TARGETPLATFORM" in \
    "linux/amd64") cp /home/fluvio/target/x86_64-unknown-linux-musl/release/yellowstone-grpc-source . ;; \
    "linux/arm64") cp /home/fluvio/target/aarch64-unknown-linux-musl/release/yellowstone-grpc-source . ;; \
    esac

FROM scratch

COPY --from=builder /home/fluvio/yellowstone-grpc-source /yellowstone-grpc-source
CMD ["/yellowstone-grpc-source"]