use clap::Parser;

#[derive(Debug, Parser)]
pub enum Command {
    Encode(EncodeCommand),
    Decode(DecodeCommand),
}

#[derive(Debug, Parser)]
pub struct EncodeCommand {
    /// The hex string to encode.
    pub hex: String,

    /// The bech32 prefix to use.
    #[clap(short, long, default_value = "xch")]
    pub prefix: String,
}

#[derive(Debug, Parser)]
pub struct DecodeCommand {
    /// The bech32 string to decode.
    pub bech32: String,
}
