use std::{
    sync::Arc, 
    time::{Duration, Instant}, 
};
use tokio::sync::mpsc;
use fluvio::{Fluvio, RecordKey, TopicProducerPool, metadata::topic::TopicSpec};
use futures::{sink::SinkExt, stream::StreamExt};
use log::{info, error};
use serde_json;
use serde_yaml;
//use anyhow::Result;

const EPOCH_SIZE: u64 = 432000;
const CHANNEL_SIZE: usize = 50_000;

// Internal modules
mod config;
mod formatters;
mod metrics;
use config::YellowstoneGrpcConfig;
use metrics::{Metrics, MetricsReporter};
//use yellowstone_grpc_proto::prost::Message;

// Yellowstone-specific imports
use yellowstone_grpc_client::GeyserGrpcClient;
use yellowstone_grpc_proto::
    prelude::{
        subscribe_update::UpdateOneof, CommitmentLevel,
        SubscribeUpdateTransaction, SubscribeUpdateAccount,
        SubscribeUpdateBlockMeta
    }
;

#[derive(Debug)]
enum ProcessingMessage {
    Transaction(SubscribeUpdateTransaction),
    //Account(SubscribeUpdateAccount),
    BlockMetadata(SubscribeUpdateBlockMeta),
    Shutdown,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let _ = env_logger::try_init();
    info!("Starting Yellowstone gRPC to Fluvio Streamer");

    let config_yaml = match std::fs::read_to_string("config.yaml") {
        Ok(contents) => contents,
        Err(e) => {
            error!("Failed to read config file: {:?}", e);
            std::process::exit(1);
        }
    };
    let config: YellowstoneGrpcConfig = serde_yaml::from_str(&config_yaml)?;

    println!("Loaded config: {:?}", config);

    // Connect to Fluvio
    let fluvio = Fluvio::connect().await?; 
    let topic_name = &config.yellowstone_grpc.topic_name;  
ensure_topic_exists(&fluvio, topic_name).await?;
let producer = Arc::new(fluvio.topic_producer(topic_name).await.expect("Failed to create producer"));

    let runtime = tokio::runtime::Runtime::new()?; 
    let _guard = runtime.enter(); 

    // Initialize metrics
    let metrics_config = config.get_metrics_config(); 
    let metrics = if metrics_config.enabled {
        info!("Metrics enabled, sending to: {}", metrics_config.endpoint);
        let metrics = Arc::new(Metrics::new());
        let reporter = MetricsReporter::new(metrics.clone(), metrics_config);
        reporter.start().await;
        Some(metrics)
    } else {
        info!("Metrics disabled");
        None
    };

    // ✅ Connect to Yellowstone gRPC
    let mut client = GeyserGrpcClient::build_from_shared(config.yellowstone_grpc.endpoint.clone())?
        .x_token(config.yellowstone_grpc.x_token.clone())?
        .connect_timeout(Duration::from_secs(10))
        .timeout(Duration::from_secs(10))
        .max_decoding_message_size(config.yellowstone_grpc.max_decoding_message_size as usize)
        .connect()
        .await?;
    

    // ✅ Subscribe to the gRPC stream
    let (mut subscribe_tx, mut stream) = client.subscribe().await?;

    let commitment = config
        .yellowstone_grpc.commitment
        .as_ref()
        .map(|s| CommitmentLevel::from_str_name(s).unwrap_or(CommitmentLevel::Processed));

    let subscribe_request = config::get_subscribe_request(&config.yellowstone_grpc.filters, commitment).await?;

    subscribe_tx.send(subscribe_request).await?;

    // Create channels for different message types
    let (tx_sender, tx_receiver) = mpsc::channel::<ProcessingMessage>(CHANNEL_SIZE);

    // Spawn processor tasks
    let tx_handle = tokio::spawn(transaction_processor(
        tx_receiver,
        Arc::clone(&producer),
        config.yellowstone_grpc.format.clone(),
        metrics,
    ));

    let mut last_slot_check = Instant::now();

    // Main processing loop with graceful shutdown handling
    let processing = async {
        while let Some(message) = stream.next().await {
            match message {
                Ok(update) => match update.update_oneof {
                    Some(UpdateOneof::BlockMeta(msg)) => {
                        if last_slot_check.elapsed() >= Duration::from_secs(5) {
                            let slot = msg.slot;

                            // Get all slot info, handling potential errors
                            let processed = client.get_slot(Some(CommitmentLevel::Processed)).await.ok();
                            let confirmed = client.get_slot(Some(CommitmentLevel::Confirmed)).await.ok();
                            let finalized = client.get_slot(Some(CommitmentLevel::Finalized)).await.ok();

                            if let (Some(processed), Some(confirmed), Some(finalized)) = (processed, confirmed, finalized) {
                                let processed_diff = processed.slot as i64 - slot as i64;
                                let confirmed_diff = confirmed.slot as i64 - slot as i64;
                                let finalized_diff = finalized.slot as i64 - slot as i64;

                                info!(
                                    "Last slot processed: {}, Mainnet watermarks: [P: {}, C: {}, F: {}], Deltas: [P: {}, C: {}, F: {}]", 
                                    format_slot_yellow(slot), 
                                    format_slot(processed.slot), format_slot(confirmed.slot), format_slot(finalized.slot),
                                    format_delta(processed_diff), format_delta(confirmed_diff), format_delta(finalized_diff)
                                );
                            }
                            last_slot_check = Instant::now();
                        }

                        if tx_sender.send(ProcessingMessage::BlockMetadata(msg)).await.is_err() {
                            error!("Block Metadata channel closed, shutting down");
                            break;
                        }
                    },
                    Some(UpdateOneof::Transaction(msg)) => {
                        if last_slot_check.elapsed() >= Duration::from_secs(5) {
                            let slot = msg.slot;

                            // Get all slot info, handling potential errors
                            let processed = client.get_slot(Some(CommitmentLevel::Processed)).await.ok();
                            let confirmed = client.get_slot(Some(CommitmentLevel::Confirmed)).await.ok();
                            let finalized = client.get_slot(Some(CommitmentLevel::Finalized)).await.ok();

                            if let (Some(processed), Some(confirmed), Some(finalized)) = (processed, confirmed, finalized) {
                                let processed_diff = processed.slot as i64 - slot as i64;
                                let confirmed_diff = confirmed.slot as i64 - slot as i64;
                                let finalized_diff = finalized.slot as i64 - slot as i64;

                                info!(
                                    "Last slot processed: {}, Mainnet watermarks: [P: {}, C: {}, F: {}], Deltas: [P: {}, C: {}, F: {}]", 
                                    format_slot_yellow(slot), 
                                    format_slot(processed.slot), format_slot(confirmed.slot), format_slot(finalized.slot),
                                    format_delta(processed_diff), format_delta(confirmed_diff), format_delta(finalized_diff)
                                );
                            }
                            last_slot_check = Instant::now();
                        }

                        if tx_sender.send(ProcessingMessage::Transaction(msg)).await.is_err() {
                            error!("Block Metadata channel closed, shutting down");
                            break;
                        }
                    },
                    // Handle other message types similarly...
                    _ => {},
                },
                Err(e) => {
                    error!("Error: {:?}", e);
                },
            }
        }
    };

    processing.await; 

    info!("Initiating graceful shutdown"); 
    let _ = tx_sender.send(ProcessingMessage::Shutdown).await; 

    let _ = tx_handle.await;

    Ok(())
}

async fn ensure_topic_exists(fluvio: &Fluvio, topic_name: &str) -> anyhow::Result<()> {
    let admin = fluvio.admin().await;
    
    // Get all topics and check if ours exists
    let topics = admin.all::<TopicSpec>().await?;
    let topic_exists = topics.iter().any(|t| t.name == topic_name);

    if topic_exists {
        info!("Topic '{}' already exists. Skipping creation.", topic_name);
    } else {
        info!("Topic '{}' does not exist. Creating it now...", topic_name);
        let topic_spec = TopicSpec::new_computed(1, 1, None); // 1 partition, 1 replica
        admin.create(topic_name.to_string(), false, topic_spec).await?;
        info!("Topic '{}' created successfully!", topic_name);
    }

    Ok(())
}

/// Process transactions & send to Fluvio
async fn transaction_processor(
    mut rx: mpsc::Receiver<ProcessingMessage>,
    producer: Arc<TopicProducerPool>,
    _format: String,
    _metrics: Option<Arc<Metrics>>,
) {
    while let Some(msg) = rx.recv().await {
        match msg {
            ProcessingMessage::Transaction(tx) => {
                if let Some(transaction) = tx.transaction.clone() {
                    let key: RecordKey = bs58::encode(&transaction.signature).into_string().into();
                    let json_value = formatters::format_transaction(tx).unwrap_or_else(|_| serde_json::json!({}));

                    if let Err(e) = producer.send(key, json_value.to_string().into_bytes()).await {
                        error!("Error processing transaction: {:?}", e);
                        error!("Fatal error processing transaction. Exiting...");
                        std::process::exit(1);
                    }
                }
            }
            ProcessingMessage::BlockMetadata(block_meta) => {
                let key: RecordKey = bs58::encode(&block_meta.blockhash).into_string().into();
                let json_value = formatters::format_block_meta(block_meta).unwrap_or_else(|_| serde_json::json!({}));

                if let Err(e) = producer.send(key, json_value.to_string().into_bytes()).await {
                    error!("Error processing block metadata: {:?}", e);
                    error!("Fatal error processing block metadata. Exiting...");
                    std::process::exit(1);
                }
            }
            ProcessingMessage::Shutdown => break,
        }
    }
}

fn format_delta(delta: i64) -> String {
    if delta < 0 {
        format!("\x1b[32m{}\x1b[0m", delta) // Green color for negative
    } else {
        format!("\x1b[31m{}\x1b[0m", delta) // Red color for positive
    }
}

fn format_slot_yellow(slot: u64) -> String {
    format!("\x1b[33m{}\x1b[0m", slot) // Yellow color
}

fn format_slot(slot: u64) -> String {
    format!("\x1b[36m{}\x1b[0m", slot) // Cyan color
}