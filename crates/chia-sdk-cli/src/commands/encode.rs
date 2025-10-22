use anyhow::Result;
use chia_wallet_sdk::utils::parse_hex;

use crate::args::EncodeCommand;

pub fn encode(args: &EncodeCommand) -> Result<()> {
    let data = parse_hex(args.hex)?;

    Ok(())
}
