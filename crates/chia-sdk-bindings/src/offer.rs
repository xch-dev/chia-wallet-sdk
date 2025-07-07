use bindy::Result;
use chia_protocol::SpendBundle;

pub fn encode_offer(spend_bundle: SpendBundle) -> Result<String> {
    Ok(chia_sdk_driver::encode_offer(&spend_bundle)?)
}

pub fn decode_offer(offer: String) -> Result<SpendBundle> {
    Ok(chia_sdk_driver::decode_offer(&offer)?)
}
