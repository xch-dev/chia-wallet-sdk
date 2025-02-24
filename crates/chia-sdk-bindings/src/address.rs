use bindy::Result;
use chia_protocol::Bytes32;

#[derive(Clone)]
pub struct Address {
    pub puzzle_hash: Bytes32,
    pub prefix: String,
}

impl Address {
    pub fn encode(&self) -> Result<String> {
        Ok(chia_sdk_utils::Address::new(self.puzzle_hash, self.prefix.clone()).encode()?)
    }

    pub fn decode(address: String) -> Result<Self> {
        let info = chia_sdk_utils::Address::decode(&address)?;
        Ok(Self {
            puzzle_hash: info.puzzle_hash,
            prefix: info.prefix,
        })
    }
}
