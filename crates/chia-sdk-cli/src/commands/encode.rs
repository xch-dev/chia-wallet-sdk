use anyhow::Result;
use chia_wallet_sdk::prelude::*;

use crate::args::EncodeCommand;

pub fn encode(args: &EncodeCommand) -> Result<()> {
    let data = parse_hex(&args.hex)?;
    let address = Bech32::new(data, args.prefix.to_string()).encode()?;

    println!("{address}");

    Ok(())
}
