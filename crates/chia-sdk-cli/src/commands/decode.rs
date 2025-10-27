use anyhow::Result;
use chia_wallet_sdk::prelude::*;

use crate::args::DecodeCommand;

pub fn decode(args: &DecodeCommand) -> Result<()> {
    let address = Bech32::decode(&args.bech32)?;

    println!("{}", address.data);

    Ok(())
}
