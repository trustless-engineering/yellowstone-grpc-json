# Yellowstone gRPC to Fluvio Streamer

## Overview

This project is a **Rust-based data pipeline** that streams data from **Yellowstone gRPC** to **Fluvio**, a distributed event streaming platform. The application subscribes to updates from the Solana blockchain via gRPC and publishes them to a Fluvio topic.

## Features

- **Connects to Yellowstone gRPC** and subscribes to blockchain updates.
- **Ensures a Fluvio topic exists** before streaming data.
- **Processes transactions and block metadata** from the Solana blockchain.
- **Supports metrics collection** for performance monitoring.
- **Graceful shutdown handling** to ensure safe termination.

## Prerequisites

Before running the project, ensure you have:

- Rust (latest stable version) → [Install Rust](https://www.rust-lang.org/tools/install)
- Fluvio CLI → [Install Fluvio](https://fluvio.io/docs/getting-started/)
- A Yellowstone gRPC endpoint

## Configuration

The application reads its configuration from a **YAML file** (`config.yaml`). Below is an example configuration to stream block metadata:

```yaml
yellowstone_grpc:
  endpoint: "Your gRPC endpoint"
  x_token: null
  max_decoding_message_size: 8388608
  commitment: "PROCESSED"
  topic_name: "solana-stream"
  filters:
    blocks_meta: true
  format: "json"
  metrics:
    enabled: true
    api_token: "Bearer your-api-token"
    endpoint: "https://metrics.yourservice.com"
    interval: 10