use std::{borrow::Cow, sync::LazyLock};

use chia_protocol::{Bytes, Bytes32, Coin, CoinSpend};
use chia_sdk_types::{
    announcement_id,
    conditions::SendMessage,
    load_clvm,
    puzzles::{SingletonMember, SingletonMemberSolution},
    Compilation, Condition, Mod,
};
use chia_sha2::Sha256;
use clvm_traits::{FromClvm, ToClvm};
use clvm_utils::TreeHash;
use clvmr::{Allocator, NodePtr};

use crate::{mips_puzzle_hash, DriverError, InnerPuzzleSpend, MipsSpend, Spend, SpendContext};

use super::MofN;

// Puzzle structure
#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(curry)]
pub struct BulletinArgs {
    pub id: Bytes32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bulletin<S: BulletinSignature> {
    pub coin: Coin,
    pub id: Bytes32,
    pub message: NodePtr,
    pub signature: S,
}

static BULLETIN_MOD: LazyLock<Compilation> =
    LazyLock::new(|| load_clvm("bulletin.clsp", &[]).unwrap());

impl Mod for BulletinArgs {
    fn mod_reveal() -> Cow<'static, [u8]> {
        Cow::Owned(BULLETIN_MOD.reveal.clone())
    }

    fn mod_hash() -> TreeHash {
        BULLETIN_MOD.hash
    }
}

impl<S: BulletinSignature> Bulletin<S> {
    pub fn new(coin: Coin, id: Bytes32, message: NodePtr, signature: S) -> Self {
        Self {
            coin,
            id,
            message,
            signature,
        }
    }

    pub fn announcement_id(&self, ctx: &mut SpendContext) -> Bytes32 {
        let mut announcement_msg_hasher = Sha256::new();
        let signature_as_clvm = ctx.alloc(&self.signature);
        announcement_msg_hasher.update(ctx.tree_hash(self.message));
        announcement_msg_hasher.update(ctx.tree_hash(signature_as_clvm.unwrap()));

        announcement_id(self.coin.coin_id(), announcement_msg_hasher.finalize())
    }

    pub fn spend(&self, ctx: &mut SpendContext, coin: Coin) -> Result<(), DriverError> {
        let signature_ptr = ctx.alloc(&self.signature).unwrap();
        let puzzle = ctx.curry(BulletinArgs { id: self.id }).unwrap();
        let solution = ctx.alloc(&vec![self.message, signature_ptr]).unwrap();
        let bulletin_coin_spend =
            CoinSpend::new(coin, ctx.serialize(&puzzle)?, ctx.serialize(&solution)?);
        ctx.insert(bulletin_coin_spend);
        Ok(())
    }
}

// Signature schemes
pub trait BulletinSignature: ToClvm<Allocator> + FromClvm<Allocator> {
    fn verify(&self, coin: Coin) -> Result<(), DriverError>;
}

// Scheme - Verify by singleton that minted a CAT
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SingletonTwoOfTwo {
    pub launcher_id: Bytes32,
    pub right_side_puzhash: Bytes32,
}

impl SingletonTwoOfTwo {
    pub fn member(&self) -> SingletonMember {
        SingletonMember::new(self.launcher_id)
    }

    pub fn member_spend(
        &self,
        ctx: &mut SpendContext,
        singleton_inner_puzzle_hash: TreeHash,
    ) -> InnerPuzzleSpend {
        let singleton_member_puzzle = self.member();
        let singleton_member_solution =
            SingletonMemberSolution::new(Bytes32::from(singleton_inner_puzzle_hash), 1);
        InnerPuzzleSpend::new(
            0,
            Vec::new(),
            Spend::new(
                ctx.curry(singleton_member_puzzle).unwrap(),
                ctx.alloc(&singleton_member_solution).unwrap(),
            ),
        )
    }

    pub fn left_side_hash(&self) -> TreeHash {
        mips_puzzle_hash(0, Vec::new(), self.member().curry_tree_hash(), false)
    }

    pub fn mofn(&self) -> MofN {
        MofN::new(
            2,
            vec![
                self.left_side_hash(),
                TreeHash::from(self.right_side_puzhash),
            ],
        )
    }

    pub fn puzhash(&self) -> TreeHash {
        mips_puzzle_hash(0, Vec::new(), self.mofn().inner_puzzle_hash(), true)
    }

    pub fn send_message_condition(
        &self,
        ctx: &mut SpendContext,
        delegated_puzzle_hash: TreeHash,
        bulletin_parent_id: Bytes32,
    ) -> SendMessage<NodePtr> {
        Condition::send_message(
            0b0001_0111,
            Bytes::from(Bytes32::from(delegated_puzzle_hash)),
            vec![ctx.alloc(&bulletin_parent_id).unwrap()],
        )
        .into_send_message()
        .unwrap()
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
            self.member_spend(ctx, singleton_inner_puzzle_hash),
        );
        parent_spend.members.insert(
            TreeHash::from(self.right_side_puzhash),
            right_side_member_spend,
        );
        let m_of_n_spend = InnerPuzzleSpend::m_of_n(
            0,
            Vec::new(),
            2,
            vec![
                self.left_side_hash(),
                TreeHash::from(self.right_side_puzhash),
            ],
        );
        let m_of_n_spend_clone = m_of_n_spend.clone();
        parent_spend
            .members
            .insert(self.mofn().inner_puzzle_hash(), m_of_n_spend);
        let two_of_two_spend =
            m_of_n_spend_clone.spend(ctx, &parent_spend, &mut Vec::new(), true)?;
        let two_of_two_coin_spend = CoinSpend::new(
            coin,
            ctx.serialize(&two_of_two_spend.puzzle).unwrap(),
            ctx.serialize(&two_of_two_spend.solution).unwrap(),
        );
        ctx.insert(two_of_two_coin_spend);
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ToClvm, FromClvm)]
#[clvm(list)]
pub struct MessageFromSingletonThatMintedCAT {
    pub launcher_id: Bytes32,
    pub parent_parent_coin_id: Bytes32,
    pub parent_right_side_puzhash: Bytes32,
    pub parent_coin_amount: u64,
}

impl BulletinSignature for MessageFromSingletonThatMintedCAT {
    fn verify(&self, coin: Coin) -> Result<(), DriverError> {
        let expected_puzhash = SingletonTwoOfTwo {
            launcher_id: self.launcher_id,
            right_side_puzhash: self.parent_right_side_puzhash,
        }
        .puzhash();

        let expected_parent_coin = Coin::new(
            self.parent_parent_coin_id,
            Bytes32::from(expected_puzhash),
            self.parent_coin_amount,
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

    use chia_puzzle_types::{singleton::SingletonSolution, EveProof, Memos, Proof};
    use chia_sdk_test::Simulator;
    use chia_sdk_types::Conditions;
    use clvm_traits::clvm_quote;
    use clvm_utils::ToTreeHash;
    use rstest::rstest;

    use crate::{Launcher, Layer, SingletonLayer, SpendWithConditions, StandardLayer};

    use super::*;

    #[rstest]
    #[case::message_atom(Allocator::new().new_atom(b"foo").unwrap())]
    #[case::message_tree(Allocator::new().new_pair(Allocator::new().new_atom(b"foo").unwrap(), Allocator::new().new_atom(b"foo").unwrap()).unwrap())]
    fn test_bulletin_creation(#[case] message: NodePtr) -> anyhow::Result<()> {
        let mut sim = Simulator::new();
        let ctx = &mut SpendContext::new();

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
            right_side_puzhash: Bytes32::from(nil_member_hash),
        };
        let bulletin_parent_coin = Coin::new(
            bulletin_parent_fund.coin.coin_id(),
            Bytes32::from(two_of_two.puzhash()),
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
        let signature = MessageFromSingletonThatMintedCAT {
            launcher_id,
            parent_parent_coin_id: bulletin_parent_coin.parent_coin_info,
            parent_right_side_puzhash: Bytes32::from(nil_member_hash),
            parent_coin_amount: bulletin_parent_coin.amount,
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
            signature,
        };
        let announcement_id = bulletin.announcement_id(ctx);
        let delegated_puzzle = ctx.alloc(&clvm_quote!(Conditions::new()
            .create_coin(Bytes32::from(bulletin_puzzle_hash), 0, Memos::None)
            .assert_coin_announcement(announcement_id)))?;
        let delegated_spend = Spend::new(delegated_puzzle, NodePtr::NIL);
        two_of_two
            .spend(
                ctx,
                bulletin_parent_coin,
                delegated_spend,
                singleton_inner_puzzle_hash,
                InnerPuzzleSpend::new(0, Vec::new(), Spend::new(NodePtr::NIL, NodePtr::NIL)),
            )
            .unwrap();

        // Spend the bulletin coin as well with the message and signature
        bulletin.spend(ctx, bulletin_coin).unwrap();

        // Spend the singleton to authorize bulletin creation
        let required_send_message = two_of_two.send_message_condition(
            ctx,
            ctx.tree_hash(delegated_puzzle),
            bulletin_parent_coin.coin_id(),
        );
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
        let spends: Vec<CoinSpend> = ctx.iter().cloned().collect();
        for spend in &spends {
            let puzzle_ptr = ctx.alloc(&spend.puzzle_reveal).unwrap();
            assert_eq!(
                spend.coin.puzzle_hash,
                Bytes32::from(ctx.tree_hash(puzzle_ptr))
            );
        }
        sim.spend_coins(ctx.take(), slice::from_ref(&singleton_fund.sk))?;

        // Check that the signature verifies
        signature.verify(bulletin_coin).unwrap();

        Ok(())
    }
}
