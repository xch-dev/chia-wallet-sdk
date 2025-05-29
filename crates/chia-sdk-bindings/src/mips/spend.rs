use std::sync::{Arc, Mutex};

use bindy::Result;
use chia_bls::PublicKey;
use chia_consensus::opcodes::{
    CREATE_COIN_ANNOUNCEMENT, CREATE_PUZZLE_ANNOUNCEMENT, RECEIVE_MESSAGE, SEND_MESSAGE,
};
use chia_protocol::{Bytes, Bytes32};
use chia_sdk_driver::{self as sdk, member_puzzle_hash, InnerPuzzleSpend, MofN, SpendContext};
use chia_sdk_types::{
    puzzles::{
        BlsMember, FixedPuzzleMember, Force1of2RestrictedVariable,
        Force1of2RestrictedVariableSolution, K1Member, K1MemberPuzzleAssert,
        K1MemberPuzzleAssertSolution, K1MemberSolution, PasskeyMember, PasskeyMemberPuzzleAssert,
        PasskeyMemberPuzzleAssertSolution, PasskeyMemberSolution, PreventConditionOpcode,
        PreventMultipleCreateCoinsMod, R1Member, R1MemberPuzzleAssert,
        R1MemberPuzzleAssertSolution, R1MemberSolution, SingletonMember, SingletonMemberSolution,
        Timelock, PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH,
    },
    Mod,
};
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{K1PublicKey, K1Signature, Program, R1PublicKey, R1Signature, Spend};

use super::{convert_restrictions, MemberConfig, Vault};

#[derive(Clone)]
pub struct MipsSpend {
    pub(crate) clvm: Arc<Mutex<SpendContext>>,
    pub(crate) spend: Arc<Mutex<sdk::MipsSpend>>,
    pub(crate) coin: chia_protocol::Coin,
}

impl MipsSpend {
    pub fn spend(&self, custody_hash: TreeHash) -> Result<Spend> {
        let mut ctx = self.clvm.lock().unwrap();

        let spend = self.spend.lock().unwrap().spend(&mut ctx, custody_hash)?;

        Ok(Spend {
            puzzle: Program(self.clvm.clone(), spend.puzzle),
            solution: Program(self.clvm.clone(), spend.solution),
        })
    }

    pub fn spend_vault(&self, vault: Vault) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();
        let vault = sdk::Vault::from(vault);
        let mips_spend = self.spend.lock().unwrap();
        vault.spend(&mut ctx, &mips_spend)?;
        Ok(())
    }

    pub fn m_of_n(&self, config: MemberConfig, required: u32, items: Vec<TreeHash>) -> Result<()> {
        let restrictions = convert_restrictions(config.restrictions);

        let member = MofN::new(required as usize, items.clone());

        let member_hash = member_puzzle_hash(
            config.nonce.try_into().unwrap(),
            restrictions.clone(),
            member.inner_puzzle_hash(),
            config.top_level,
        );

        self.spend.lock().unwrap().members.insert(
            member_hash,
            InnerPuzzleSpend::m_of_n(
                config.nonce.try_into().unwrap(),
                restrictions,
                required.try_into().unwrap(),
                items,
            ),
        );

        Ok(())
    }

    pub fn k1_member(
        &self,
        config: MemberConfig,
        public_key: K1PublicKey,
        signature: K1Signature,
        fast_forward: bool,
    ) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();

        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions);

        let (member_hash, member_puzzle) = if fast_forward {
            let member = K1MemberPuzzleAssert::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, ctx.curry(member)?)
        } else {
            let member = K1Member::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, ctx.curry(member)?)
        };

        let member_hash =
            member_puzzle_hash(nonce, restrictions.clone(), member_hash, config.top_level);

        let member_solution = if fast_forward {
            ctx.alloc(&K1MemberPuzzleAssertSolution::new(
                self.coin.puzzle_hash,
                signature.0,
            ))?
        } else {
            ctx.alloc(&K1MemberSolution::new(self.coin.coin_id(), signature.0))?
        };

        self.spend.lock().unwrap().members.insert(
            member_hash,
            InnerPuzzleSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    pub fn r1_member(
        &self,
        config: MemberConfig,
        public_key: R1PublicKey,
        signature: R1Signature,
        fast_forward: bool,
    ) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();

        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions);

        let (member_hash, member_puzzle) = if fast_forward {
            let member = R1MemberPuzzleAssert::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, ctx.curry(member)?)
        } else {
            let member = R1Member::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, ctx.curry(member)?)
        };

        let member_hash =
            member_puzzle_hash(nonce, restrictions.clone(), member_hash, config.top_level);

        let member_solution = if fast_forward {
            ctx.alloc(&R1MemberPuzzleAssertSolution::new(
                self.coin.puzzle_hash,
                signature.0,
            ))?
        } else {
            ctx.alloc(&R1MemberSolution::new(self.coin.coin_id(), signature.0))?
        };

        self.spend.lock().unwrap().members.insert(
            member_hash,
            InnerPuzzleSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    pub fn bls_member(&self, config: MemberConfig, public_key: PublicKey) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();

        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions);

        let member = BlsMember::new(public_key);
        let member_hash = member.curry_tree_hash();
        let member_hash =
            member_puzzle_hash(nonce, restrictions.clone(), member_hash, config.top_level);

        let member_puzzle = ctx.curry(member)?;
        let member_solution = ctx.alloc(&NodePtr::NIL)?;

        self.spend.lock().unwrap().members.insert(
            member_hash,
            InnerPuzzleSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    pub fn passkey_member(
        &self,
        config: MemberConfig,
        public_key: R1PublicKey,
        signature: R1Signature,
        authenticator_data: Bytes,
        client_data_json: Bytes,
        challenge_index: u32,
        fast_forward: bool,
    ) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();

        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions);

        let (member_hash, member_puzzle) = if fast_forward {
            let member = PasskeyMemberPuzzleAssert::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, ctx.curry(member)?)
        } else {
            let member = PasskeyMember::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, ctx.curry(member)?)
        };

        let member_hash =
            member_puzzle_hash(nonce, restrictions.clone(), member_hash, config.top_level);

        let member_solution = if fast_forward {
            ctx.alloc(&PasskeyMemberPuzzleAssertSolution {
                authenticator_data,
                client_data_json,
                challenge_index: challenge_index.try_into().unwrap(),
                signature: signature.0,
                puzzle_hash: self.coin.puzzle_hash,
            })?
        } else {
            ctx.alloc(&PasskeyMemberSolution {
                authenticator_data,
                client_data_json,
                challenge_index: challenge_index.try_into().unwrap(),
                signature: signature.0,
                coin_id: self.coin.coin_id(),
            })?
        };

        self.spend.lock().unwrap().members.insert(
            member_hash,
            InnerPuzzleSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    pub fn singleton_member(
        &self,
        config: MemberConfig,
        launcher_id: Bytes32,
        singleton_inner_puzzle_hash: Bytes32,
        singleton_amount: u64,
    ) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();

        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions);

        let member = SingletonMember::new(launcher_id);

        let member_hash = member_puzzle_hash(
            nonce,
            restrictions.clone(),
            member.curry_tree_hash(),
            config.top_level,
        );

        let member_puzzle = ctx.curry(member)?;

        let member_solution = ctx.alloc(&SingletonMemberSolution::new(
            singleton_inner_puzzle_hash,
            singleton_amount,
        ))?;

        self.spend.lock().unwrap().members.insert(
            member_hash,
            InnerPuzzleSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    pub fn fixed_puzzle_member(
        &self,
        config: MemberConfig,
        fixed_puzzle_hash: Bytes32,
    ) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();

        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions);

        let member = FixedPuzzleMember::new(fixed_puzzle_hash);

        let member_hash = member_puzzle_hash(
            nonce,
            restrictions.clone(),
            member.curry_tree_hash(),
            config.top_level,
        );

        let member_puzzle = ctx.curry(member)?;

        self.spend.lock().unwrap().members.insert(
            member_hash,
            InnerPuzzleSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, NodePtr::NIL),
            ),
        );

        Ok(())
    }

    pub fn custom_member(&self, config: MemberConfig, spend: Spend) -> Result<()> {
        let ctx = self.clvm.lock().unwrap();

        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions);

        let member_hash = member_puzzle_hash(
            nonce,
            restrictions.clone(),
            ctx.tree_hash(spend.puzzle.1),
            config.top_level,
        );

        self.spend.lock().unwrap().members.insert(
            member_hash,
            InnerPuzzleSpend::new(
                nonce,
                restrictions,
                chia_sdk_driver::Spend {
                    puzzle: spend.puzzle.1,
                    solution: spend.solution.1,
                },
            ),
        );

        Ok(())
    }

    pub fn timelock(&self, timelock: u64) -> Result<()> {
        let restriction = Timelock::new(timelock);
        let puzzle = self.clvm.lock().unwrap().curry(restriction)?;
        self.spend.lock().unwrap().restrictions.insert(
            restriction.curry_tree_hash(),
            sdk::Spend::new(puzzle, NodePtr::NIL),
        );
        Ok(())
    }

    pub fn force_1_of_2_restricted_variable(
        &self,
        left_side_subtree_hash: Bytes32,
        nonce: u32,
        member_validator_list_hash: Bytes32,
        delegated_puzzle_validator_list_hash: Bytes32,
        new_right_side_member_hash: Bytes32,
    ) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();

        let restriction = Force1of2RestrictedVariable::new(
            left_side_subtree_hash,
            nonce.try_into().unwrap(),
            member_validator_list_hash,
            delegated_puzzle_validator_list_hash,
        );

        let puzzle = ctx.curry(restriction)?;
        let solution = ctx.alloc(&Force1of2RestrictedVariableSolution::new(
            new_right_side_member_hash,
        ))?;

        self.spend.lock().unwrap().restrictions.insert(
            restriction.curry_tree_hash(),
            sdk::Spend::new(puzzle, solution),
        );

        Ok(())
    }

    pub fn prevent_condition_opcode(&self, condition_opcode: u16) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();

        let restriction = PreventConditionOpcode::new(condition_opcode);
        let puzzle = ctx.curry(restriction)?;
        let solution = ctx.alloc(&NodePtr::NIL)?;

        self.spend.lock().unwrap().restrictions.insert(
            restriction.curry_tree_hash(),
            sdk::Spend::new(puzzle, solution),
        );

        Ok(())
    }

    pub fn prevent_multiple_create_coins(&self) -> Result<()> {
        let mut ctx = self.clvm.lock().unwrap();

        let puzzle = ctx.alloc_mod::<PreventMultipleCreateCoinsMod>()?;
        let solution = ctx.alloc(&NodePtr::NIL)?;

        self.spend.lock().unwrap().restrictions.insert(
            PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH,
            sdk::Spend::new(puzzle, solution),
        );

        Ok(())
    }

    pub fn prevent_vault_side_effects(&self) -> Result<()> {
        self.prevent_condition_opcode(CREATE_COIN_ANNOUNCEMENT)?;
        self.prevent_condition_opcode(CREATE_PUZZLE_ANNOUNCEMENT)?;
        self.prevent_condition_opcode(SEND_MESSAGE)?;
        self.prevent_condition_opcode(RECEIVE_MESSAGE)?;
        self.prevent_multiple_create_coins()?;
        Ok(())
    }
}
