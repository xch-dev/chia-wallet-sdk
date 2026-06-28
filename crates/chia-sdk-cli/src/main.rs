mod args;
mod commands;

use anyhow::Result;
use clap::Parser;

use crate::args::Command;

fn main() -> Result<()> {
    let command = Command::parse();

    match command {
        Command::Encode(args) => commands::encode(&args)?,
        Command::Decode(args) => commands::decode(&args)?,
    }

    Ok(())
}
