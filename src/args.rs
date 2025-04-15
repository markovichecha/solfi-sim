use clap::{Parser, Subcommand};

#[derive(Debug, Subcommand)]
pub enum Command {
    /// Fetch the solfi wsol/usdc pool accounts and related data
    FetchAccounts,

    /// Print slot cutoff and other metadata from fetched solfi pool data
    Cutoffs,

    /// Simulate a WSOL -> USDC swap in all the solfi wsol/usdc pools
    Simulate {
        /// Amount of SOL to swap to USDC
        #[arg(short, long)]
        amount: Option<f64>,

        /// Slot to simulate at (default: uses metadata.json)
        #[arg(short, long)]
        slot: Option<u64>,

        /// Don't print simulation errors
        #[arg(long)]
        ignore_errors: bool,
    },
}

#[derive(Debug, Parser)]
#[clap(name = "app", version)]
pub struct App {
    #[clap(subcommand)]
    pub command: Command,
}
