use chia_protocol::{Bytes, Bytes32, Coin};
use chia_sdk_types::{
    announcement_id,
    conditions::SendMessage,
    puzzles::{BulletinArgs, BulletinSolution, SingletonMember, SingletonMemberSolution},
    Mod,
};
use chia_sha2::Sha256;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    mips_puzzle_hash, DriverError, HashedPtr, InnerPuzzleSpend, MipsSpend, Spend, SpendContext,
};

use super::MofN;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bulletin<S> {
    pub coin: Coin,
    pub id: Bytes32,
    pub message: HashedPtr,
    pub signature: S,
}

impl<S> Bulletin<S> {
    pub fn new(coin: Coin, id: Bytes32, message: HashedPtr, signature: S) -> Self {
        Self {
            coin,
            id,
            message,
            signature,
        }
    }

    pub fn announcement_id(&self) -> Bytes32
    where
        S: ToTreeHash,
    {
        let mut announcement_msg_hasher = Sha256::new();
        announcement_msg_hasher.update(self.message.tree_hash());
        announcement_msg_hasher.update(self.signature.tree_hash());
        announcement_id(self.coin.coin_id(), announcement_msg_hasher.finalize())
    }

    pub fn spend(&self, ctx: &mut SpendContext, coin: Coin) -> Result<(), DriverError>
    where
        S: ToClvm<Allocator>,
    {
        let puzzle = ctx.curry(BulletinArgs::new(self.id))?;
        let solution = ctx.alloc(&BulletinSolution::new(self.message, &self.signature))?;
        ctx.spend(coin, Spend::new(puzzle, solution))?;
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SingletonTwoOfTwo {
    pub launcher_id: Bytes32,
    pub right_side_puzzle_hash: Bytes32,
}

impl SingletonTwoOfTwo {
    pub fn member(&self) -> SingletonMember {
        SingletonMember::new(self.launcher_id)
    }

    pub fn member_spend(
        &self,
        ctx: &mut SpendContext,
        singleton_inner_puzzle_hash: TreeHash,
    ) -> Result<InnerPuzzleSpend, DriverError> {
        let singleton_member_puzzle = self.member();
        let singleton_member_solution =
            SingletonMemberSolution::new(Bytes32::from(singleton_inner_puzzle_hash), 1);
        Ok(InnerPuzzleSpend::new(
            0,
            Vec::new(),
            Spend::new(
                ctx.curry(singleton_member_puzzle)?,
                ctx.alloc(&singleton_member_solution)?,
            ),
        ))
    }

    pub fn left_side_hash(&self) -> TreeHash {
        mips_puzzle_hash(0, Vec::new(), self.member().curry_tree_hash(), false)
    }

    pub fn m_of_n(&self) -> MofN {
        MofN::new(
            2,
            vec![
                self.left_side_hash(),
                TreeHash::from(self.right_side_puzzle_hash),
            ],
        )
    }

    pub fn send_message_condition(
        &self,
        ctx: &mut SpendContext,
        delegated_puzzle_hash: TreeHash,
        bulletin_parent_id: Bytes32,
    ) -> Result<SendMessage<NodePtr>, DriverError> {
        Ok(SendMessage::new(
            0b0001_0111,
            Bytes::from(Bytes32::from(delegated_puzzle_hash)),
            vec![ctx.alloc(&bulletin_parent_id)?],
        ))
    }

    pub fn spend(
        &self,
        ctx: &mut SpendContext,
        coin: Coin,
        delegated_spend: Spend,
        singleton_inner_puzzle_hash: TreeHash,
        right_side_member_spend: InnerPuzzleSpend,
    ) -> Result<(), DriverError> {
        let mut parent_spend = MipsSpend::new(delegated_spend);
        parent_spend.members.insert(
            self.left_side_hash(),
            self.member_spend(ctx, singleton_inner_puzzle_hash)?,
        );
        parent_spend.members.insert(
            TreeHash::from(self.right_side_puzzle_hash),
            right_side_member_spend,
        );
        let m_of_n_spend = InnerPuzzleSpend::m_of_n(
            0,
            Vec::new(),
            2,
            vec![
                self.left_side_hash(),
                TreeHash::from(self.right_side_puzzle_hash),
            ],
        );
        let m_of_n_spend_clone = m_of_n_spend.clone();
        parent_spend
            .members
            .insert(self.m_of_n().inner_puzzle_hash(), m_of_n_spend);
        let two_of_two_spend =
            m_of_n_spend_clone.spend(ctx, &parent_spend, &mut Vec::new(), true)?;
        ctx.spend(coin, two_of_two_spend)?;
        Ok(())
    }
}

impl ToTreeHash for SingletonTwoOfTwo {
    fn tree_hash(&self) -> TreeHash {
        mips_puzzle_hash(0, Vec::new(), self.m_of_n().inner_puzzle_hash(), true)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct BulletinParentInfo {
    pub launcher_id: Bytes32,
    pub parent_parent_coin_id: Bytes32,
    pub parent_right_side_puzzle_hash: Bytes32,
    pub parent_amount: u64,
}

impl BulletinParentInfo {
    pub fn verify(&self, coin: Coin) -> Result<(), DriverError> {
        let expected_puzhash = SingletonTwoOfTwo {
            launcher_id: self.launcher_id,
            right_side_puzzle_hash: self.parent_right_side_puzzle_hash,
        }
        .tree_hash();

        let expected_parent_coin = Coin::new(
            self.parent_parent_coin_id,
            Bytes32::from(expected_puzhash),
            self.parent_amount,
        );

        if expected_parent_coin.coin_id() != coin.parent_coin_info {
            return Err(DriverError::BulletSignatureVerificationFailed);
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::slice;

    use anyhow::Result;
    use chia_puzzle_types::{singleton::SingletonSolution, EveProof, Memos, Proof};
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use clvm_traits::clvm_quote;
    use clvm_utils::ToTreeHash;
    use clvmr::Allocator;
    use rstest::rstest;

    use crate::{Launcher, Layer, SingletonLayer, SpendWithConditions, StandardLayer};

    use super::*;

    #[rstest]
    #[case::message_atom(b"foo")]
    #[case::message_tree((b"foo", b"foo"))]
    fn test_bulletin_creation(#[case] message: impl ToClvm<Allocator>) -> Result<()> {
        let mut sim = Simulator::new();

        let ctx = &mut SpendContext::new();

        let message = ctx.alloc_hashed(&message)?;

        // Stuff we would know ahead of time
        let hypothetical_asset_id = Bytes32::from([0; 32]);
        let bulletin_puzzle_hash = BulletinArgs {
            id: hypothetical_asset_id,
        }
        .curry_tree_hash();

        // Make the singleton
        let singleton_fund = sim.bls(1);
        let singleton_fund_puzzle = StandardLayer::new(singleton_fund.pk);
        let singleton_inner_puzzle_hash = singleton_fund_puzzle.tree_hash();
        let launcher = Launcher::new(singleton_fund.coin.coin_id(), 1);
        let launcher_id = launcher.coin().coin_id();
        let (conditions, singleton) =
            launcher.spend(ctx, Bytes32::from(singleton_inner_puzzle_hash), ())?;
        singleton_fund_puzzle.spend(ctx, singleton_fund.coin, conditions)?;

        // Make the bulletin parent
        let bulletin_parent_fund = sim.bls(0);
        let bulletin_fund_puzzle = StandardLayer::new(bulletin_parent_fund.pk);
        let nil_hash = ctx.tree_hash(NodePtr::NIL);
        let nil_member_hash = mips_puzzle_hash(0, Vec::new(), nil_hash, false);
        let two_of_two = SingletonTwoOfTwo {
            launcher_id,
            right_side_puzzle_hash: Bytes32::from(nil_member_hash),
        };
        let bulletin_parent_coin = Coin::new(
            bulletin_parent_fund.coin.coin_id(),
            two_of_two.tree_hash().into(),
            0,
        );
        bulletin_fund_puzzle.spend(
            ctx,
            bulletin_parent_fund.coin,
            Conditions::new().create_coin(bulletin_parent_coin.puzzle_hash, 0, Memos::None),
        )?;

        // Process the singleton and bulletin parent creations
        sim.spend_coins(
            ctx.take(),
            &[singleton_fund.sk.clone(), bulletin_parent_fund.sk],
        )?;

        // Spend the bulletin parent into the bulletin
        let bulletin_parent_info = BulletinParentInfo {
            launcher_id,
            parent_parent_coin_id: bulletin_parent_coin.parent_coin_info,
            parent_right_side_puzzle_hash: Bytes32::from(nil_member_hash),
            parent_amount: bulletin_parent_coin.amount,
        };
        let bulletin_coin = Coin {
            parent_coin_info: bulletin_parent_coin.coin_id(),
            puzzle_hash: Bytes32::from(bulletin_puzzle_hash),
            amount: bulletin_parent_coin.amount,
        };
        let bulletin = Bulletin {
            coin: bulletin_coin,
            id: hypothetical_asset_id,
            message,
            signature: bulletin_parent_info,
        };
        let announcement_id = bulletin.announcement_id();
        let delegated_puzzle = ctx.alloc(&clvm_quote!(Conditions::new()
            .create_coin(Bytes32::from(bulletin_puzzle_hash), 0, Memos::None)
            .assert_coin_announcement(announcement_id)))?;
        let delegated_spend = Spend::new(delegated_puzzle, NodePtr::NIL);
        two_of_two.spend(
            ctx,
            bulletin_parent_coin,
            delegated_spend,
            singleton_inner_puzzle_hash,
            InnerPuzzleSpend::new(0, Vec::new(), Spend::new(NodePtr::NIL, NodePtr::NIL)),
        )?;

        // Spend the bulletin coin as well with the message and signature
        bulletin.spend(ctx, bulletin_coin)?;

        // Spend the singleton to authorize bulletin creation
        let required_send_message = two_of_two.send_message_condition(
            ctx,
            ctx.tree_hash(delegated_puzzle),
            bulletin_parent_coin.coin_id(),
        )?;
        let inner_solution = singleton_fund_puzzle
            .spend_with_conditions(
                ctx,
                Conditions::new()
                    .create_coin(singleton_fund.puzzle_hash, 1, Memos::None)
                    .with(required_send_message),
            )?
            .solution;
        let singleton_spend =
            SingletonLayer::new(launcher_id, singleton_fund_puzzle.construct_puzzle(ctx)?)
                .construct_coin_spend(
                    ctx,
                    singleton,
                    SingletonSolution {
                        lineage_proof: Proof::Eve(EveProof {
                            parent_parent_coin_info: singleton_fund.coin.coin_id(),
                            parent_amount: 1,
                        }),
                        amount: singleton.amount,
                        inner_solution,
                    },
                )?;

        ctx.insert(singleton_spend);

        // Check the spends
        let spends = ctx.take();

        for spend in &spends {
            let puzzle_ptr = ctx.alloc(&spend.puzzle_reveal)?;
            assert_eq!(spend.coin.puzzle_hash, ctx.tree_hash(puzzle_ptr).into());
        }
        sim.spend_coins(spends, slice::from_ref(&singleton_fund.sk))?;

        // Check that the signature verifies
        bulletin_parent_info.verify(bulletin_coin)?;

        Ok(())
    }
}
