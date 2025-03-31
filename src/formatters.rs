use anyhow::Context;
use serde_json::{json, Value};
use solana_sdk::{hash::Hash, pubkey::Pubkey, signature::Signature};
use solana_transaction_status::UiTransactionEncoding;
use yellowstone_grpc_proto::{convert_from, geyser::{
    CommitmentLevel, SubscribeUpdateAccount, SubscribeUpdateAccountInfo, SubscribeUpdateBlock, SubscribeUpdateBlockMeta, SubscribeUpdateEntry, SubscribeUpdateSlot, SubscribeUpdateTransaction, SubscribeUpdateTransactionInfo, SubscribeUpdateTransactionStatus
}};
use log::info;
use base64;

use crate::EPOCH_SIZE;

pub fn format_account(update: SubscribeUpdateAccount) -> anyhow::Result<Value> {
    let Some(account_info) = update.account else {
        return Err(anyhow::anyhow!("Missing account info"));
    };

    let base64_data = base64::encode(&account_info.data);

    Ok(json!({
        "pubkey": bs58::encode(&account_info.pubkey).into_string(),
        "lamports": account_info.lamports,
        "owner": bs58::encode(&account_info.owner).into_string(),
        "rent_epoch": account_info.rent_epoch,
        "slot": update.slot,
        "data": base64_data,
        "txn_signature": account_info.txn_signature.as_ref().map(|sig| bs58::encode(sig).into_string()),
    }))
}

pub fn format_slot(msg: SubscribeUpdateSlot) -> anyhow::Result<Value> {
    let status = CommitmentLevel::try_from(msg.status).context("failed to decode commitment")?;

    Ok(json!({
        "slot": msg.slot,
        "status": status.as_str_name(),
    }))
}

pub fn format_transaction(msg: SubscribeUpdateTransaction) -> anyhow::Result<Value> {
    let tx = msg
        .transaction
        .ok_or(anyhow::anyhow!("no transaction in the message"))?;

    let encoded = convert_from::create_tx_with_meta(tx)
        .map_err(|error| anyhow::anyhow!(error))
        .context("invalid tx with meta")?
        .encode(UiTransactionEncoding::JsonParsed, Some(u8::MAX), true)
        .context("failed to encode transaction")?;

    let mut value = json!(encoded);
    
    // Ensure all transactions include these fields
    value["slot"] = json!(msg.slot);
    value["epoch"] = json!(msg.slot / EPOCH_SIZE);

    // 🔥 Debugging: Log to check output
    info!("Formatted transaction: {}", value.to_string());

    Ok(value)
}

// pub fn format_transaction_status(msg: SubscribeUpdateTransactionStatus) -> anyhow::Result<Value> {
//     Ok(json!({
//         "slot": msg.slot,
//         "epoch": msg.slot / EPOCH_SIZE,
//         "signature": Signature::try_from(msg.signature.as_slice()).context("invalid signature")?.to_string(),
//         "isVote": msg.is_vote,
//         "index": msg.index,
//         "err": if let Some(err) = msg.err {
//             convert_from::create_tx_error(Some(&err))
//                 .map_err(|error| anyhow::anyhow!(error))
//                 .context("invalid error")?
//         } else {
//             convert_from::create_tx_error(None)
//                 .map_err(|error| anyhow::anyhow!(error))
//                 .context("invalid error")?
//         },
//     }))
// }

// pub fn format_entry(msg: SubscribeUpdateEntry) -> anyhow::Result<Value> {
//     Ok(json!({
//         "slot": msg.slot,
//         "index": msg.index,
//         "numHashes": msg.num_hashes,
//         "hash": Hash::new_from_array(<[u8; 32]>::try_from(msg.hash.as_slice()).context("invalid entry hash")?).to_string(),
//     }))
// }

pub fn format_block_meta(msg: SubscribeUpdateBlockMeta) -> anyhow::Result<Value> {
    Ok(json!({
        "slot": msg.slot,
        "blockhash": msg.blockhash,
        "rewards": msg.rewards.map(|rewards| convert_from::create_rewards_obj(rewards).ok()),
        "blockTime": msg.block_time.map(|obj| obj.timestamp),
        "blockHeight": msg.block_height.map(|obj| obj.block_height),
        "parentSlot": msg.parent_slot,
        "parentBlockhash": msg.parent_blockhash,
        "executedTransactionCount": msg.executed_transaction_count,
        "entriesCount": msg.entries_count,
    }))
}

// pub fn format_block(msg: SubscribeUpdateBlock) -> anyhow::Result<Value> {
//     Ok(json!({
//         "slot": msg.slot,
//         "blockhash": msg.blockhash,
//         "rewards": if let Some(rewards) = msg.rewards {
//             Some(convert_from::create_rewards_obj(rewards).map_err(|error| anyhow::anyhow!(error))?)
//         } else {
//             None
//         },
//         "blockTime": msg.block_time.map(|obj| obj.timestamp),
//         "blockHeight": msg.block_height.map(|obj| obj.block_height),
//         "parentSlot": msg.parent_slot,
//         "parentBlockhash": msg.parent_blockhash,
//         "executedTransactionCount": msg.executed_transaction_count,
//         "transactions": msg.transactions.into_iter().map(create_pretty_transaction).collect::<anyhow::Result<Value, _>>()?,
//         "updatedAccountCount": msg.updated_account_count,
//         "accounts": msg.accounts.into_iter().map(create_pretty_account).collect::<anyhow::Result<Value, _>>()?,
//         "entriesCount": msg.entries_count,
//         "entries": msg.entries.into_iter().map(create_pretty_entry).collect::<anyhow::Result<Value, _>>()?,
//     }))
// }

fn create_pretty_account(account: SubscribeUpdateAccountInfo) -> anyhow::Result<Value> {
    Ok(json!({
        "pubkey": Pubkey::try_from(account.pubkey).map_err(|_| anyhow::anyhow!("invalid account pubkey"))?.to_string(),
        "lamports": account.lamports,
        "owner": Pubkey::try_from(account.owner).map_err(|_| anyhow::anyhow!("invalid account owner"))?.to_string(),
        "executable": account.executable,
        "rentEpoch": account.rent_epoch,
        "data": hex::encode(account.data),
        "writeVersion": account.write_version,
        "txnSignature": account.txn_signature.map(|sig| bs58::encode(sig).into_string()),
    }))
}

// fn create_pretty_transaction(tx: SubscribeUpdateTransactionInfo) -> anyhow::Result<Value> {
//     let (is_vote, is_error) = if tx.is_vote {
//         (true, false)
//     } else {
//         (false, tx.meta.as_ref().unwrap().err.is_some())
//     };

//     Ok(json!({
//         "signature": Signature::try_from(tx.signature.as_slice()).context("invalid signature")?.to_string(),
//         "isVote": is_vote,
//         "isError": is_error,
//         "tx": convert_from::create_tx_with_meta(tx)
//             .map_err(|error| anyhow::anyhow!(error))
//             .context("invalid tx with meta")?
//             .encode(UiTransactionEncoding::JsonParsed, Some(u8::MAX), true)
//             .context("failed to encode transaction")?,
//     }))
// }

// fn create_pretty_entry(msg: SubscribeUpdateEntry) -> anyhow::Result<Value> {
//     Ok(json!({
//         "slot": msg.slot,
//         "index": msg.index,
//         "numHashes": msg.num_hashes,
//         "hash": Hash::new_from_array(<[u8; 32]>::try_from(msg.hash.as_slice()).context("invalid entry hash")?).to_string(),
//         "executedTransactionCount": msg.executed_transaction_count,
//         "startingTransactionIndex": msg.starting_transaction_index,
//     }))
// }
