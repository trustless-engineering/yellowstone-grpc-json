use chrono::Utc;
use reqwest::{Client, header};
use serde_json::{json, Value};
use std::time::Duration;
use tokio::time;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use log::{info, warn, error, debug};

/// Metrics configuration
#[derive(Debug, Clone)]
pub struct MetricsConfig {
    /// Enable metrics reporting
    pub enabled: bool,
    /// BetterStack API token
    pub api_token: String,
    /// BetterStack API endpoint
    pub endpoint: String,
    /// Reporting interval in seconds
    pub interval: u64,
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            api_token: String::new(),
            endpoint: "https://s1231159.eu-nbg-2.betterstackdata.com/metrics".to_string(),
            interval: 10,
        }
    }
}

/// Metric type for storing counter values
#[derive(Debug)]
pub struct Metrics {
    processed_transactions: AtomicU64,
    processed_accounts: AtomicU64,
    errors: AtomicU64,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            processed_transactions: AtomicU64::new(0),
            processed_accounts: AtomicU64::new(0),
            errors: AtomicU64::new(0),
        }
    }

    /// Increment the transaction counter
    pub fn increment_transactions(&self) {
        self.processed_transactions.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the accounts counter
    pub fn increment_accounts(&self) {
        self.processed_accounts.fetch_add(1, Ordering::Relaxed);
    }

    /// Increment the errors counter
    pub fn increment_errors(&self) {
        self.errors.fetch_add(1, Ordering::Relaxed);
    }

    /// Get current transaction count
    pub fn transactions(&self) -> u64 {
        self.processed_transactions.load(Ordering::Relaxed)
    }

    /// Get current account count
    pub fn accounts(&self) -> u64 {
        self.processed_accounts.load(Ordering::Relaxed)
    }

    /// Get current error count
    pub fn errors(&self) -> u64 {
        self.errors.load(Ordering::Relaxed)
    }
}

/// BetterStack metrics reporter
pub struct MetricsReporter {
    metrics: Arc<Metrics>,
    config: MetricsConfig,
    client: Client,
    last_transactions: AtomicU64,
    last_accounts: AtomicU64,
    last_errors: AtomicU64,
}

impl MetricsReporter {
    /// Create a new metrics reporter
    pub fn new(metrics: Arc<Metrics>, config: MetricsConfig) -> Self {
        let client = Client::builder()
            .timeout(Duration::from_secs(5))
            .build()
            .expect("Failed to create HTTP client");
        
        Self {
            metrics,
            config,
            client,
            last_transactions: AtomicU64::new(0),
            last_accounts: AtomicU64::new(0),
            last_errors: AtomicU64::new(0),
        }
    }

    /// Start the metrics reporter in the background
    pub async fn start(self) {
        if !self.config.enabled {
            return;
        }

        // Ensure interval is at least 10 seconds
        let interval_secs = self.config.interval.max(10);
        info!("Starting metrics reporter with interval of {} seconds", interval_secs);
        
        tokio::spawn(async move {
            let mut interval = time::interval(Duration::from_secs(interval_secs));
            
            loop {
                interval.tick().await;
                if let Err(e) = self.report_metrics().await {
                    error!("Error reporting metrics: {}", e);
                }
            }
        });
    }

    /// Report metrics to BetterStack
    async fn report_metrics(&self) -> Result<(), reqwest::Error> {
        let metrics = &self.metrics;
        let timestamp = Utc::now().format("%Y-%m-%d %T UTC").to_string();
        
        // Calculate deltas since last report
        let current_transactions = metrics.transactions();
        let current_accounts = metrics.accounts();
        let current_errors = metrics.errors();
        
        let last_transactions = self.last_transactions.swap(current_transactions, Ordering::Relaxed);
        let last_accounts = self.last_accounts.swap(current_accounts, Ordering::Relaxed);
        let last_errors = self.last_errors.swap(current_errors, Ordering::Relaxed);
        
        // Calculate deltas (handle case where counters might reset)
        let transactions_delta = if current_transactions >= last_transactions {
            current_transactions - last_transactions
        } else {
            current_transactions
        };
        
        let accounts_delta = if current_accounts >= last_accounts {
            current_accounts - last_accounts
        } else {
            current_accounts
        };
        
        let errors_delta = if current_errors >= last_errors {
            current_errors - last_errors
        } else {
            current_errors
        };
        
        debug!("Reporting metrics - transactions delta: {}, accounts delta: {}, errors delta: {}", 
               transactions_delta, accounts_delta, errors_delta);
        
        // Report transactions metric
        self.send_metric(
            "yellowstone_processed_transactions",
            transactions_delta,
            &timestamp,
        ).await?;
        
        // Report accounts metric
        self.send_metric(
            "yellowstone_processed_accounts", 
            accounts_delta,
            &timestamp,
        ).await?;
        
        // Report errors metric
        self.send_metric(
            "yellowstone_errors",
            errors_delta,
            &timestamp,
        ).await?;
        
        Ok(())
    }

    /// Send a single metric to BetterStack
    async fn send_metric(&self, name: &str, value: u64, timestamp: &str) -> Result<(), reqwest::Error> {
        let payload = json!({
            "dt": timestamp,
            "name": name,
            "gauge": {
                "value": value
            }
        });

        let mut headers = header::HeaderMap::new();
        let auth_value = format!("Bearer {}", self.config.api_token);
        headers.insert(
            header::AUTHORIZATION,
            header::HeaderValue::from_str(&auth_value).unwrap(),
        );

        self.client
            .post(&self.config.endpoint)
            .headers(headers)
            .json(&payload)
            .send()
            .await?;

        Ok(())
    }
} 