#[derive(Default)]
pub struct Simulator(chia_sdk_test::Simulator);

impl Simulator {
    pub fn new() -> Self {
        Self::default()
    }
}
