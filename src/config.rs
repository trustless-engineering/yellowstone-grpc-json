use std::{collections::HashMap, fs::File};

use serde::Deserialize;
use yellowstone_grpc_proto::prelude::*;
use yellowstone_grpc_proto::prelude::{
    subscribe_request_filter_accounts_filter::Filter as AccountsFilterOneof,
    subscribe_request_filter_accounts_filter_lamports::Cmp as AccountsFilterLamports,
    subscribe_request_filter_accounts_filter_memcmp::Data as AccountsFilterMemcmpOneof,
    CommitmentLevel,
};

// Add metrics module
use crate::metrics::MetricsConfig;

type SlotsFilterMap = HashMap<String, SubscribeRequestFilterSlots>;
type AccountFilterMap = HashMap<String, SubscribeRequestFilterAccounts>;
type TransactionsFilterMap = HashMap<String, SubscribeRequestFilterTransactions>;
type TransactionsStatusFilterMap = HashMap<String, SubscribeRequestFilterTransactions>;
type EntryFilterMap = HashMap<String, SubscribeRequestFilterEntry>;
type BlocksFilterMap = HashMap<String, SubscribeRequestFilterBlocks>;
type BlocksMetaFilterMap = HashMap<String, SubscribeRequestFilterBlocksMeta>;

#[derive(Debug, Deserialize)]
pub(crate) struct YellowstoneGrpcConfig {
    pub yellowstone_grpc: YellowstoneGrpc,  
}

#[derive(Debug, Deserialize)]
pub(crate) struct YellowstoneGrpc {
    pub endpoint: String,
    pub x_token: Option<String>,
    pub max_decoding_message_size: u32,
    pub commitment: Option<String>,
    pub filters: Filters,
    pub format: String,
    pub metrics: Option<MetricsConfigWrapper>,
    pub topic_name: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct Filters {
    /// Subscribe on accounts updates
    accounts: Option<bool>,

    accounts_nonempty_txn_signature: Option<bool>,

    /// Filter by Account Pubkey
    accounts_account: Option<Vec<String>>,

    /// Path to a JSON array of account addresses
    accounts_account_path: Option<String>,

    /// Filter by Owner Pubkey
    accounts_owner: Option<Vec<String>>,

    /// Filter by Offset and Data, format: `offset,data in base58`
    accounts_memcmp: Option<Vec<String>>,

    /// Filter by Data size
    accounts_datasize: Option<u64>,

    /// Filter valid token accounts
    accounts_token_account_state: Option<bool>,

    /// Filter by lamports, format: `eq:42` / `ne:42` / `lt:42` / `gt:42`
    accounts_lamports: Option<Vec<String>>,

    /// Receive only part of updated data account, format: `offset,size`
    accounts_data_slice: Option<Vec<String>>,

    /// Subscribe on slots updates
    slots: Option<bool>,

    /// Filter slots by commitment
    slots_filter_by_commitment: Option<bool>,

    /// Subscribe on transactions updates
    transactions: Option<bool>,

    /// Filter vote transactions
    transactions_vote: Option<bool>,

    /// Filter failed transactions
    transactions_failed: Option<bool>,

    /// Filter by transaction signature
    transactions_signature: Option<String>,

    /// Filter included account in transactions
    transactions_account_include: Option<Vec<String>>,

    /// Filter excluded account in transactions
    transactions_account_exclude: Option<Vec<String>>,

    /// Filter required account in transactions
    transactions_account_required: Option<Vec<String>>,

    /// Subscribe on transactions_status updates
    transactions_status: Option<bool>,

    /// Filter vote transactions for transactions_status
    transactions_status_vote: Option<bool>,

    /// Filter failed transactions for transactions_status
    transactions_status_failed: Option<bool>,

    /// Filter by transaction signature for transactions_status
    transactions_status_signature: Option<String>,

    /// Filter included account in transactions for transactions_status
    transactions_status_account_include: Option<Vec<String>>,

    /// Filter excluded account in transactions for transactions_status
    transactions_status_account_exclude: Option<Vec<String>>,

    /// Filter required account in transactions for transactions_status
    transactions_status_account_required: Option<Vec<String>>,

    entries: Option<bool>   ,

    /// Subscribe on block updates
    blocks: Option<bool>,

    /// Filter included account in transactions
    blocks_account_include: Option<Vec<String>>,

    /// Include transactions to block message
    blocks_include_transactions: Option<bool>,

    /// Include accounts to block message
    blocks_include_accounts: Option<bool>,

    /// Include entries to block message
    blocks_include_entries: Option<bool>,

    /// Subscribe on block meta updates (without transactions)
    blocks_meta: Option<bool>,

    /// Send ping in subscribe request
    ping: Option<i32>,
}

#[derive(Debug, Deserialize)]
pub struct MetricsConfigWrapper {
    /// Enable metrics reporting
    pub enabled: Option<bool>,
    /// BetterStack API token
    pub api_token: Option<String>,
    /// BetterStack API endpoint
    pub endpoint: Option<String>,
    /// Reporting interval in seconds
    pub interval: Option<u64>,
}

impl YellowstoneGrpcConfig {
    /// Get metrics configuration
    pub fn get_metrics_config(&self) -> MetricsConfig {
        let default_config = MetricsConfig::default();
        
        if let Some(metrics_config) = &self.yellowstone_grpc.metrics {
            MetricsConfig {
                enabled: metrics_config.enabled.unwrap_or(default_config.enabled),
                api_token: metrics_config.api_token.clone().unwrap_or(default_config.api_token),
                endpoint: metrics_config.endpoint.clone().unwrap_or(default_config.endpoint),
                interval: metrics_config.interval.unwrap_or(default_config.interval),
            }
        } else {
            default_config
        }
    }
}

pub(crate) async fn get_subscribe_request(args: &Filters, commitment: Option<CommitmentLevel>) -> Result<SubscribeRequest, anyhow::Error> {
    let mut accounts: AccountFilterMap = HashMap::new();
    if args.accounts.unwrap_or(false) {
        let mut accounts_account = args.accounts_account.clone().unwrap_or_default();
        if let Some(path) = args.accounts_account_path.clone() {
            let accounts = tokio::task::block_in_place(|| {
                let file = File::open(&path)?;
                Ok::<Vec<String>, anyhow::Error>(serde_json::from_reader(file)?)
            })?;
            accounts_account.extend(accounts);
        }

        let mut filters = vec![];
        for filter in args.accounts_memcmp.iter().flatten() {
            let parts: Vec<&str> = filter.split(',').collect();
            match parts.as_slice() {
                [offset, data] => {
                    filters.push(SubscribeRequestFilterAccountsFilter {
                        filter: Some(AccountsFilterOneof::Memcmp(
                            SubscribeRequestFilterAccountsFilterMemcmp {
                                offset: offset.parse()
                                    .map_err(|_| anyhow::anyhow!("invalid offset"))?,
                                data: Some(AccountsFilterMemcmpOneof::Base58(
                                    data.trim().to_string(),
                                )),
                            },
                        )),
                    });
                },
                _ => anyhow::bail!("invalid memcmp"),
            }
        }
        if let Some(datasize) = args.accounts_datasize {
            filters.push(SubscribeRequestFilterAccountsFilter {
                filter: Some(AccountsFilterOneof::Datasize(datasize)),
            });
        }
        if args.accounts_token_account_state.unwrap_or(false) {
            filters.push(SubscribeRequestFilterAccountsFilter {
                filter: Some(AccountsFilterOneof::TokenAccountState(true)),
            });
        }
        for filter in args.accounts_lamports.iter().flatten() {
            let parts: Vec<&str> = filter.split(':').collect();
            match parts.as_slice() {
                [cmp, value] => {
                    let value: u64 = value.parse()
                        .map_err(|_| anyhow::anyhow!("invalid lamports value"))?;
                    filters.push(SubscribeRequestFilterAccountsFilter {
                        filter: Some(AccountsFilterOneof::Lamports(
                            SubscribeRequestFilterAccountsFilterLamports {
                                cmp: Some(match *cmp {
                                    "eq" => AccountsFilterLamports::Eq(value),
                                    "ne" => AccountsFilterLamports::Ne(value),
                                    "lt" => AccountsFilterLamports::Lt(value),
                                    "gt" => AccountsFilterLamports::Gt(value),
                                    _ => {
                                        anyhow::bail!("invalid lamports filter: {cmp}")
                                    },
                                }),
                            },
                        )),
                    });
                },
                _ => anyhow::bail!("invalid lamports format"),
            }
        }

        accounts.insert(
            "client".to_owned(),
            SubscribeRequestFilterAccounts {
                nonempty_txn_signature: args.accounts_nonempty_txn_signature,
                account: accounts_account,
                owner: args.accounts_owner.clone().unwrap_or_default(),
                filters,
            },
        );
    }

    let mut slots: SlotsFilterMap = HashMap::new();
    if args.slots.unwrap_or(false) {
        slots.insert(
            "client".to_owned(),
            SubscribeRequestFilterSlots {
                filter_by_commitment: Some(args.slots_filter_by_commitment.unwrap_or(false)),
                interslot_updates: None, 
            }
            
        );
    }

    let mut transactions: TransactionsFilterMap = HashMap::new();
    if args.transactions.unwrap_or(false) {
        transactions.insert(
            "client".to_string(),
            SubscribeRequestFilterTransactions {
                vote: args.transactions_vote,
                failed: args.transactions_failed,
                signature: args.transactions_signature.clone(),
                account_include: args.transactions_account_include.clone().unwrap_or_default(),
                account_exclude: args.transactions_account_exclude.clone().unwrap_or_default(),
                account_required: args.transactions_account_required.clone().unwrap_or_default(),
            },
        );
    }

    let mut transactions_status: TransactionsStatusFilterMap = HashMap::new();
    if args.transactions_status.unwrap_or(false) {
        transactions_status.insert(
            "client".to_string(),
            SubscribeRequestFilterTransactions {
                vote: args.transactions_status_vote,
                failed: args.transactions_status_failed,
                signature: args.transactions_status_signature.clone(),
                account_include: args.transactions_status_account_include.clone().unwrap_or_default(),
                account_exclude: args.transactions_status_account_exclude.clone().unwrap_or_default(),
                account_required: args.transactions_status_account_required.clone().unwrap_or_default(),
            },
        );
    }

    let mut entries: EntryFilterMap = HashMap::new();
    if args.entries.unwrap_or(false) {
        entries.insert("client".to_owned(), SubscribeRequestFilterEntry {});
    }

    let mut blocks: BlocksFilterMap = HashMap::new();
    if args.blocks.unwrap_or(false) {
        blocks.insert(
            "client".to_owned(),
            SubscribeRequestFilterBlocks {
                account_include: args.blocks_account_include.clone().unwrap_or_default(),
                include_transactions: args.blocks_include_transactions,
                include_accounts: args.blocks_include_accounts,
                include_entries: args.blocks_include_entries,
            },
        );
    }

    let mut blocks_meta: BlocksMetaFilterMap = HashMap::new();
    if args.blocks_meta.unwrap_or(false) {
        blocks_meta.insert("client".to_owned(), SubscribeRequestFilterBlocksMeta {});
    }

    let mut accounts_data_slice = Vec::new();
    for data_slice in args.accounts_data_slice.iter().flatten() {
        let parts: Vec<&str> = data_slice.split(',').collect();
        match parts.as_slice() {
            [offset, length] => {
                let offset: u64 = offset.parse()?;
                let length: u64 = length.parse()?;
                accounts_data_slice.push(SubscribeRequestAccountsDataSlice { offset, length });
            },
            _ => anyhow::bail!("invalid data_slice format"),
        }
    }

    let ping = args.ping.map(|id| SubscribeRequestPing { id });

    Ok(SubscribeRequest {
        from_slot: None,
        slots,
        accounts,
        transactions,
        transactions_status,
        entry: entries,
        blocks,
        blocks_meta,
        commitment: commitment.map(|x| x as i32),
        accounts_data_slice,
        ping,
    })
}
