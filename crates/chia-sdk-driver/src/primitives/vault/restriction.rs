use chia_sdk_types::Timelock;

#[derive(Debug, Clone, Copy)]
pub enum Restriction {
    Timelock(Timelock),
}
