#![allow(unused)]
use bitcoin::amount;
use bitcoin::hex::DisplayHex;
use bitcoincore_rpc::bitcoin::Amount;
use bitcoincore_rpc::{Auth, Client, RpcApi};
use serde::Deserialize;
use serde_json::json;
use std::fs::{self, File};
use std::io::Write;
use std::str::FromStr;

// Node access params
const RPC_URL: &str = "http://127.0.0.1:18443"; // Default regtest RPC port
const RPC_USER: &str = "alice";
const RPC_PASS: &str = "password";

// You can use calls not provided in RPC lib API using the generic `call` function.
// An example of using the `send` RPC call, which doesn't have exposed API.
// You can also use serde_json `Deserialize` derivation to capture the returned json result.
fn send(rpc: &Client, addr: &str) -> bitcoincore_rpc::Result<String> {
    let args = [
        json!([{addr : 100 }]), // recipient address
        json!(null),            // conf target
        json!(null),            // estimate mode
        json!(null),            // fee rate in sats/vb
        json!(null),            // Empty option object
    ];

    #[derive(Deserialize)]
    struct SendResult {
        complete: bool,
        txid: String,
    }
    let send_result = rpc.call::<SendResult>("send", &args)?;
    assert!(send_result.complete);
    Ok(send_result.txid)
}

fn generate_wallet(rpc: &Client, wallet: &str) -> bitcoincore_rpc::Result<()> {
    match rpc.create_wallet(wallet, None, None, None, None) {
        Ok(_) => Ok(()),
        Err(e) => {
            if e.to_string().contains("already exists") {
                match rpc.load_wallet(wallet) {
                    Ok(_) => Ok(()),
                    Err(load_err) => {
                        if load_err.to_string().contains("already loaded") {
                            Ok(())
                        } else {
                            Err(e)
                        }
                    }
                }
            } else {
                Err(e)
            }
        }
    }
}

fn main() -> bitcoincore_rpc::Result<()> {
    // Connect to Bitcoin Core RPC
    let rpc = Client::new(
        RPC_URL,
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    // Get blockchain info
    // commented it out because it was showing some Json error on my end
    // let blockchain_info = rpc.get_blockchain_info()?;
    // println!("Blockchain Info: {:?}", blockchain_info);

    // Create/Load the wallets, named 'Miner' and 'Trader'.
    // Have logic to optionally create/load them if they do not exist or not loaded already.
    generate_wallet(&rpc, "Miner")?;
    generate_wallet(&rpc, "Trader")?;

    // Generate spendable balances in the Miner wallet. How many blocks needs to be mined?
    // 101 blocks needs to be mined because the first 100 blocks are not spendable output which
    // are called the coinbase output but after the 100th block, it finally gets me a spendable
    // positive value
    let miner_rpc = Client::new(
        &format!("{}/wallet/Miner", RPC_URL).to_owned(),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;

    let miner_address = miner_rpc.get_new_address(Some("Mining Reward"), None)?;
    rpc.generate_to_address(101, miner_address.assume_checked_ref())?;
    println!("{}", miner_rpc.get_balance(None, None)?); 

    // Load Trader wallet and generate a new address
    let trader_rpc = Client::new(
        &format!("{}/wallet/Trader", RPC_URL).to_owned(),
        Auth::UserPass(RPC_USER.to_owned(), RPC_PASS.to_owned()),
    )?;
    let trader_address = trader_rpc.get_new_address(Some("Received"), None)?;

    // Send 20 BTC from Miner to Trader
    let trader_amount = Amount::from_btc(20.0)?;
    let txid = miner_rpc.send_to_address(
        trader_address.assume_checked_ref(),
        trader_amount,
        None,
        None,
        None,
        None,
        None,
        None,
    )?;

    // Check transaction in mempool
    rpc.get_mempool_entry(&txid)?;

    // Mine 1 block to confirm the transaction
    let block = rpc.generate_to_address(1, miner_address.assume_checked_ref())?;

    // Extract all required transaction details
    let tx = miner_rpc.get_transaction(&txid, None)?;
    let block_hash = match tx.info.blockhash {
        Some(val) => val,
        None => return Err(bitcoincore_rpc::Error::UnexpectedStructure),
    };
    let block = rpc.get_block_info(&block_hash)?;

    let decode_tx = miner_rpc.decode_raw_transaction(&tx.hex, None)?;
    let raw_tx = miner_rpc.get_raw_transaction(&txid, None)?;
    let mut trader_amount = 0.0;
    let mut trader_out_address = String::new();
    let mut change = 0.0;
    let mut change_address = String::new();

    for output in decode_tx.vout {
       let amount = output.value.to_btc();
        let address = match output.script_pub_key.address {
            Some(val) => val.assume_checked_ref().to_string(),
            None => String::new(),
        };
       if trader_address.assume_checked_ref().to_string() == address {
            trader_amount = amount
        } else {
            change = amount;
            change_address = address;
        }
    }

    let vin = &raw_tx.input[0];
    let prev_txid = vin.previous_output.txid;
    let prev_vout = vin.previous_output.vout;
    let prev_tx = rpc.get_raw_transaction(&prev_txid, None)?;
    let input_amount = prev_tx.output[prev_vout as usize].value.to_btc();
    let fees = match tx.fee {
        Some(val) => -val.to_btc(),
        None => 0.0,
    };

    // Write the data to ../out.txt in the specified format given in readme.md
    let output = format!(
        "{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}\n{}",
        &txid.to_string(),
        miner_address.assume_checked_ref(),
        input_amount,
        trader_address.assume_checked_ref(),
        trader_amount,
        change_address,
        change,
        fees,
        block.height,
        block_hash.to_raw_hash()
    );
    println!("{}", output);

    fs::write("../out.txt", output)?;

    Ok(())
}
