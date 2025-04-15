use crate::constants::SOLFI_PROGRAM;
use solana_pubkey::Pubkey;
use solana_sdk::instruction::{AccountMeta, Instruction};
use solana_sdk::sysvar;
use spl_associated_token_account::get_associated_token_address;

fn create_instruction_data(discriminator: u8, amount_in: u64) -> Vec<u8> {
    let mut buffer = Vec::with_capacity(9);
    buffer.push(discriminator);
    buffer.extend_from_slice(&amount_in.to_le_bytes());
    buffer.resize(18, 0);
    buffer
}

pub fn create_swap_ix(
    market: &Pubkey,
    user: &Pubkey,
    from: &Pubkey,
    to: &Pubkey,
    amount: u64,
) -> Instruction {
    Instruction {
        program_id: SOLFI_PROGRAM,
        accounts: vec![
            AccountMeta::new(*user, true),
            AccountMeta::new(*market, false),
            AccountMeta::new(get_associated_token_address(market, from), false),
            AccountMeta::new(get_associated_token_address(market, to), false),
            AccountMeta::new(get_associated_token_address(user, from), false),
            AccountMeta::new(get_associated_token_address(user, to), false),
            AccountMeta::new_readonly(spl_token::id(), false),
            AccountMeta::new_readonly(sysvar::instructions::id(), false),
        ],
        data: create_instruction_data(7, amount),
    }
}
