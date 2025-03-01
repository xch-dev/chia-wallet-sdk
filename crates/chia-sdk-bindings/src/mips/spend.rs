use std::sync::{Arc, Mutex, RwLock};

use bindy::Result;
use chia_bls::PublicKey;
use chia_protocol::{Bytes, Bytes32};
use chia_sdk_driver::{self as sdk, member_puzzle_hash, MemberSpend, MofN, SpendContext};
use chia_sdk_types::{
    BlsMember, FixedPuzzleMember, Force1of2RestrictedVariable, Force1of2RestrictedVariableSolution,
    Mod, PasskeyMember, PasskeyMemberPuzzleAssert, PasskeyMemberPuzzleAssertSolution,
    PasskeyMemberSolution, PreventConditionOpcode, Secp256k1Member, Secp256k1MemberPuzzleAssert,
    Secp256k1MemberPuzzleAssertSolution, Secp256k1MemberSolution, Secp256r1Member,
    Secp256r1MemberPuzzleAssert, Secp256r1MemberPuzzleAssertSolution, Secp256r1MemberSolution,
    SingletonMember, SingletonMemberSolution, Timelock, PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH,
};
use clvm_utils::TreeHash;
use clvmr::NodePtr;

use crate::{K1PublicKey, K1Signature, Program, R1PublicKey, R1Signature, Spend};

use super::{convert_restrictions, MemberConfig, Vault};

#[derive(Clone)]
pub struct MipsSpend {
    pub(crate) clvm: Arc<RwLock<SpendContext>>,
    pub(crate) spend: Arc<Mutex<sdk::MipsSpend>>,
    pub(crate) coin: chia_protocol::Coin,
}

impl MipsSpend {
    pub fn spend(&self, custody_hash: TreeHash) -> Result<Spend> {
        let mut ctx = self.clvm.write().unwrap();

        let spend = self.spend.lock().unwrap().spend(&mut ctx, custody_hash)?;

        Ok(Spend {
            puzzle: Program(self.clvm.clone(), spend.puzzle),
            solution: Program(self.clvm.clone(), spend.solution),
        })
    }

    pub fn spend_vault(&self, vault: Vault) -> Result<()> {
        let mut ctx = self.clvm.write().unwrap();
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
            MemberSpend::m_of_n(
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
        let mut ctx = self.clvm.write().unwrap();

        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions);

        let (member_hash, member_puzzle) = if fast_forward {
            let member = Secp256k1MemberPuzzleAssert::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, ctx.curry(member)?)
        } else {
            let member = Secp256k1Member::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, ctx.curry(member)?)
        };

        let member_hash =
            member_puzzle_hash(nonce, restrictions.clone(), member_hash, config.top_level);

        let member_solution = if fast_forward {
            ctx.alloc(&Secp256k1MemberPuzzleAssertSolution::new(
                self.coin.puzzle_hash,
                signature.0,
            ))?
        } else {
            ctx.alloc(&Secp256k1MemberSolution::new(
                self.coin.coin_id(),
                signature.0,
            ))?
        };

        self.spend.lock().unwrap().members.insert(
            member_hash,
            MemberSpend::new(
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
        let mut ctx = self.clvm.write().unwrap();

        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions);

        let (member_hash, member_puzzle) = if fast_forward {
            let member = Secp256r1MemberPuzzleAssert::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, ctx.curry(member)?)
        } else {
            let member = Secp256r1Member::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, ctx.curry(member)?)
        };

        let member_hash =
            member_puzzle_hash(nonce, restrictions.clone(), member_hash, config.top_level);

        let member_solution = if fast_forward {
            ctx.alloc(&Secp256r1MemberPuzzleAssertSolution::new(
                self.coin.puzzle_hash,
                signature.0,
            ))?
        } else {
            ctx.alloc(&Secp256r1MemberSolution::new(
                self.coin.coin_id(),
                signature.0,
            ))?
        };

        self.spend.lock().unwrap().members.insert(
            member_hash,
            MemberSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    pub fn bls_member(&self, config: MemberConfig, public_key: PublicKey) -> Result<()> {
        let mut ctx = self.clvm.write().unwrap();

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
            MemberSpend::new(
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
        let mut ctx = self.clvm.write().unwrap();

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
            MemberSpend::new(
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
        let mut ctx = self.clvm.write().unwrap();

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
            MemberSpend::new(
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
        let mut ctx = self.clvm.write().unwrap();

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
            MemberSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, NodePtr::NIL),
            ),
        );

        Ok(())
    }

    pub fn custom_member(&self, config: MemberConfig, spend: Spend) -> Result<()> {
        let ctx = self.clvm.read().unwrap();

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
            MemberSpend::new(
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
        let puzzle = self.clvm.write().unwrap().curry(restriction)?;
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
        let mut ctx = self.clvm.write().unwrap();

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
        let mut ctx = self.clvm.write().unwrap();

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
        let mut ctx = self.clvm.write().unwrap();

        let puzzle = ctx.prevent_multiple_create_coins_puzzle()?;
        let solution = ctx.alloc(&NodePtr::NIL)?;

        self.spend.lock().unwrap().restrictions.insert(
            PREVENT_MULTIPLE_CREATE_COINS_PUZZLE_HASH,
            sdk::Spend::new(puzzle, solution),
        );

        Ok(())
    }

    pub fn prevent_side_effects(&self) -> Result<()> {
        self.prevent_condition_opcode(60)?;
        self.prevent_condition_opcode(62)?;
        self.prevent_condition_opcode(66)?;
        self.prevent_condition_opcode(67)?;
        self.prevent_multiple_create_coins()?;
        Ok(())
    }
}
