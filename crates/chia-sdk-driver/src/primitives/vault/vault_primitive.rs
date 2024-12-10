use chia_protocol::{Bytes32, Coin};

use super::{Member, PuzzleWithRestrictions};

#[derive(Debug, Clone)]
pub struct Vault {
    pub coin: Coin,
    pub launcher_id: Bytes32,
    pub custody: PuzzleWithRestrictions<Member>,
}

impl Vault {
    pub fn new(coin: Coin, launcher_id: Bytes32, custody: PuzzleWithRestrictions<Member>) -> Self {
        Self {
            coin,
            launcher_id,
            custody,
        }
    }
}

#[cfg(test)]
mod tests {
    use chia_sdk_test::Simulator;

    use crate::{Launcher, SpendContext, StandardLayer};

    use super::*;

    #[test]
    fn test_single_sig() -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

        let (sk, pk, _puzzle_hash, coin) = sim.new_p2(1)?;
        let p2 = StandardLayer::new(pk);

        let custody = PuzzleWithRestrictions::top_level(0, Vec::new(), Member::bls(pk));
        let (mint_vault, _vault) = Launcher::new(coin.coin_id(), 1).mint_vault(ctx, custody, ())?;
        p2.spend(ctx, coin, mint_vault)?;

        sim.spend_coins(ctx.take(), &[sk])?;

        Ok(())
    }
}
