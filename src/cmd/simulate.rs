use crate::constants::{SOLFI_MARKETS, SOLFI_PROGRAM, USDC, WSOL};
use crate::swap::create_swap_ix;
use crate::types::{AccountWithAddress, FetchMetadata};
use crate::utils::token_balance;
use csv::WriterBuilder;
use eyre::eyre;
use litesvm::LiteSVM;
use solana_keypair::Keypair;
use solana_message::Message;
use solana_sdk::native_token::{lamports_to_sol, sol_to_lamports};
use solana_sdk::program_pack::Pack;
use solana_sdk::rent::Rent;
use solana_signer::Signer;
use solana_system_interface::instruction::transfer;
use solana_transaction::Transaction;
use spl_associated_token_account::get_associated_token_address;
use spl_associated_token_account::instruction::create_associated_token_account_idempotent;
use spl_token::instruction::sync_native;
use std::io::stdout;

const SOLFI_PROGRAM_PATH: &str = "data/solfi.so";
const DEFAULT_SWAP_AMOUNT: f64 = 10.0;
const USDC_DECIMALS: i32 = 6;

pub fn simulate(amount: Option<f64>, slot: Option<u64>, ignore_errors: bool) -> eyre::Result<()> {
    let user_keypair = Keypair::new();
    let user = user_keypair.pubkey();
    let mut svm =
        LiteSVM::new().with_sysvars().with_precompiles().with_sigverify(true).with_spl_programs();

    for acct in AccountWithAddress::read_all()? {
        svm.set_account(acct.address, acct.account)?;
    }
    svm.add_program_from_file(SOLFI_PROGRAM, SOLFI_PROGRAM_PATH)?;
    if let Some(slot) = slot.or(FetchMetadata::read().map(|m| m.slot_lower)) {
        svm.warp_to_slot(slot);
    }

    let swap_amount_in_lamports = sol_to_lamports(amount.unwrap_or(DEFAULT_SWAP_AMOUNT));
    let airdrop_amount = swap_amount_in_lamports * 4 + sol_to_lamports(1.0); // 4 pools + some buffer
    svm.airdrop(&user, airdrop_amount).map_err(|e| eyre!("failed to airdrop: {}", e.err))?;

    let wsol_ata = get_associated_token_address(&user, &WSOL);
    let usdc_ata = get_associated_token_address(&user, &USDC);
    let ata_rent = Rent::default().minimum_balance(spl_token::state::Account::LEN);

    for market in SOLFI_MARKETS {
        let usdc_starting = token_balance(&svm, &usdc_ata);
        let tx = Transaction::new(
            &[&user_keypair],
            Message::new(
                &[
                    create_associated_token_account_idempotent(
                        &user,
                        &user,
                        &WSOL,
                        &spl_token::id(),
                    ),
                    create_associated_token_account_idempotent(
                        &user,
                        &user,
                        &USDC,
                        &spl_token::id(),
                    ),
                    transfer(&user, &wsol_ata, swap_amount_in_lamports + ata_rent),
                    sync_native(&spl_token::id(), &wsol_ata)?,
                    create_swap_ix(market, &user, &WSOL, &USDC, swap_amount_in_lamports),
                ],
                Some(&user),
            ),
            svm.latest_blockhash(),
        );

        let sol_in = lamports_to_sol(swap_amount_in_lamports);
        let mut wtr = WriterBuilder::new().has_headers(false).from_writer(stdout());

        match svm.send_transaction(tx) {
            Ok(_) => {
                let usdc_out = token_balance(&svm, &usdc_ata) - usdc_starting;
                wtr.serialize(SwapResult {
                    market: market.to_string(),
                    sol_in,
                    usdc_out: Some(usdc_out as f64 / 10f64.powi(USDC_DECIMALS)),
                    error: String::new(),
                })?;
            }
            Err(err) => {
                if !ignore_errors {
                    wtr.serialize(SwapResult {
                        market: market.to_string(),
                        sol_in,
                        usdc_out: None,
                        error: err.err.to_string(),
                    })?;
                }
            }
        }
        wtr.flush()?;
    }

    Ok(())
}

#[derive(serde::Serialize)]
struct SwapResult {
    market: String,
    sol_in: f64,
    usdc_out: Option<f64>,
    error: String,
}
