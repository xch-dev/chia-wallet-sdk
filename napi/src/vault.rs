use chia::{clvm_utils::TreeHash, protocol};
use chia_wallet_sdk::{
    self as sdk, member_puzzle_hash, BlsMember, FixedPuzzleMember, MemberSpend, Mod, MofN,
    P2SingletonMessageArgs, PasskeyMember, PasskeyMemberPuzzleAssert,
    PasskeyMemberPuzzleAssertSolution, PasskeyMemberSolution, Recovery, RecoverySolution,
    Secp256k1Member, Secp256k1MemberPuzzleAssert, Secp256k1MemberPuzzleAssertSolution,
    Secp256k1MemberSolution, Secp256r1Member, Secp256r1MemberPuzzleAssert,
    Secp256r1MemberPuzzleAssertSolution, Secp256r1MemberSolution, SingletonMember,
    SingletonMemberSolution, Timelock,
};
use clvmr::NodePtr;
use napi::bindgen_prelude::*;

use crate::{
    traits::{js_err, FromJs, IntoJs, IntoRust},
    ClvmAllocator, Coin, K1PublicKey, K1Signature, LineageProof, Program, PublicKey, R1PublicKey,
    R1Signature, Spend,
};

#[napi(object)]
pub struct Vault {
    pub coin: Coin,
    pub launcher_id: Uint8Array,
    pub proof: LineageProof,
    pub custody_hash: Uint8Array,
}

#[napi]
pub fn child_vault(vault: Vault, custody_hash: Uint8Array) -> Result<Vault> {
    let vault: sdk::Vault = vault.into_rust()?;
    vault.child(custody_hash.into_rust()?).into_js()
}

impl IntoJs<Vault> for sdk::Vault {
    fn into_js(self) -> Result<Vault> {
        Ok(Vault {
            coin: self.coin.into_js()?,
            launcher_id: self.launcher_id.into_js()?,
            proof: self.proof.into_js()?,
            custody_hash: self.custody_hash.into_js()?,
        })
    }
}

impl FromJs<Vault> for sdk::Vault {
    fn from_js(vault: Vault) -> Result<Self> {
        Ok(sdk::Vault {
            coin: vault.coin.into_rust()?,
            launcher_id: vault.launcher_id.into_rust()?,
            proof: vault.proof.into_rust()?,
            custody_hash: vault.custody_hash.into_rust()?,
        })
    }
}

#[napi(object)]
pub struct VaultMint {
    pub parent_conditions: Vec<ClassInstance<Program>>,
    pub vault: Vault,
}

#[napi]
pub struct VaultSpend {
    pub(crate) spend: sdk::VaultSpend,
    pub(crate) coin: protocol::Coin,
}

#[napi]
impl VaultSpend {
    #[napi(constructor)]
    pub fn new(delegated_spend: Spend, coin: Coin) -> Result<Self> {
        Ok(Self {
            spend: sdk::VaultSpend::new(delegated_spend.into_rust()?),
            coin: coin.into_rust()?,
        })
    }

    #[napi]
    pub fn spend_m_of_n(
        &mut self,
        config: MemberConfig,
        required: u32,
        items: Vec<Uint8Array>,
    ) -> Result<()> {
        let restrictions = convert_restrictions(config.restrictions)?;
        let items = items
            .into_iter()
            .map(IntoRust::into_rust)
            .collect::<Result<Vec<_>>>()?;

        let member = MofN::new(required.try_into().unwrap(), items.clone());

        let member_hash = member_puzzle_hash(
            config.nonce.try_into().unwrap(),
            restrictions.clone(),
            member.inner_puzzle_hash(),
            config.top_level,
        );

        self.spend.members.insert(
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

    #[napi]
    pub fn spend_k1(
        &mut self,
        clvm: &mut ClvmAllocator,
        config: MemberConfig,
        public_key: ClassInstance<K1PublicKey>,
        signature: ClassInstance<K1Signature>,
        fast_forward: bool,
    ) -> Result<()> {
        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions)?;

        let (member_hash, member_puzzle) = if fast_forward {
            let member = Secp256k1MemberPuzzleAssert::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, clvm.0.curry(member).map_err(js_err)?)
        } else {
            let member = Secp256k1Member::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, clvm.0.curry(member).map_err(js_err)?)
        };

        let member_hash =
            member_puzzle_hash(nonce, restrictions.clone(), member_hash, config.top_level);

        let member_solution = if fast_forward {
            clvm.0
                .alloc(&Secp256k1MemberPuzzleAssertSolution::new(
                    self.coin.puzzle_hash,
                    signature.0,
                ))
                .map_err(js_err)?
        } else {
            clvm.0
                .alloc(&Secp256k1MemberSolution::new(
                    self.coin.coin_id(),
                    signature.0,
                ))
                .map_err(js_err)?
        };

        self.spend.members.insert(
            member_hash,
            MemberSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    #[napi]
    pub fn spend_r1(
        &mut self,
        clvm: &mut ClvmAllocator,
        config: MemberConfig,
        public_key: ClassInstance<R1PublicKey>,
        signature: ClassInstance<R1Signature>,
        fast_forward: bool,
    ) -> Result<()> {
        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions)?;

        let (member_hash, member_puzzle) = if fast_forward {
            let member = Secp256r1MemberPuzzleAssert::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, clvm.0.curry(member).map_err(js_err)?)
        } else {
            let member = Secp256r1Member::new(public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, clvm.0.curry(member).map_err(js_err)?)
        };

        let member_hash =
            member_puzzle_hash(nonce, restrictions.clone(), member_hash, config.top_level);

        let member_solution = if fast_forward {
            clvm.0
                .alloc(&Secp256r1MemberPuzzleAssertSolution::new(
                    self.coin.puzzle_hash,
                    signature.0,
                ))
                .map_err(js_err)?
        } else {
            clvm.0
                .alloc(&Secp256r1MemberSolution::new(
                    self.coin.coin_id(),
                    signature.0,
                ))
                .map_err(js_err)?
        };

        self.spend.members.insert(
            member_hash,
            MemberSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    #[napi]
    pub fn spend_bls(
        &mut self,
        clvm: &mut ClvmAllocator,
        config: MemberConfig,
        public_key: ClassInstance<PublicKey>,
    ) -> Result<()> {
        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions)?;

        let member = BlsMember::new(public_key.0);
        let member_hash = member.curry_tree_hash();
        let member_hash =
            member_puzzle_hash(nonce, restrictions.clone(), member_hash, config.top_level);

        let member_puzzle = clvm.0.curry(member).map_err(js_err)?;
        let member_solution = clvm.0.alloc(&NodePtr::NIL).map_err(js_err)?;

        self.spend.members.insert(
            member_hash,
            MemberSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    #[napi]
    #[allow(clippy::too_many_arguments)]
    pub fn spend_passkey(
        &mut self,
        clvm: &mut ClvmAllocator,
        config: MemberConfig,
        genesis_challenge: Uint8Array,
        public_key: ClassInstance<R1PublicKey>,
        signature: ClassInstance<R1Signature>,
        authenticator_data: Uint8Array,
        client_data_json: Uint8Array,
        challenge_index: u32,
        fast_forward: bool,
    ) -> Result<()> {
        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions)?;

        let (member_hash, member_puzzle) = if fast_forward {
            let member =
                PasskeyMemberPuzzleAssert::new(genesis_challenge.into_rust()?, public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, clvm.0.curry(member).map_err(js_err)?)
        } else {
            let member = PasskeyMember::new(genesis_challenge.into_rust()?, public_key.0);
            let tree_hash = member.curry_tree_hash();
            (tree_hash, clvm.0.curry(member).map_err(js_err)?)
        };

        let member_hash =
            member_puzzle_hash(nonce, restrictions.clone(), member_hash, config.top_level);

        let member_solution = if fast_forward {
            clvm.0
                .alloc(&PasskeyMemberPuzzleAssertSolution {
                    authenticator_data: authenticator_data.into_rust()?,
                    client_data_json: client_data_json.into_rust()?,
                    challenge_index: challenge_index.try_into().unwrap(),
                    signature: signature.0,
                    puzzle_hash: self.coin.puzzle_hash,
                })
                .map_err(js_err)?
        } else {
            clvm.0
                .alloc(&PasskeyMemberSolution {
                    authenticator_data: authenticator_data.into_rust()?,
                    client_data_json: client_data_json.into_rust()?,
                    challenge_index: challenge_index.try_into().unwrap(),
                    signature: signature.0,
                    coin_id: self.coin.coin_id(),
                })
                .map_err(js_err)?
        };

        self.spend.members.insert(
            member_hash,
            MemberSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    #[napi]
    pub fn spend_singleton(
        &mut self,
        clvm: &mut ClvmAllocator,
        config: MemberConfig,
        launcher_id: Uint8Array,
        singleton_inner_puzzle_hash: Uint8Array,
        singleton_amount: BigInt,
    ) -> Result<()> {
        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions)?;

        let member = SingletonMember::new(launcher_id.into_rust()?);

        let member_hash = member_puzzle_hash(
            nonce,
            restrictions.clone(),
            member.curry_tree_hash(),
            config.top_level,
        );

        let member_puzzle = clvm.0.curry(member).map_err(js_err)?;

        let member_solution = clvm
            .0
            .alloc(&SingletonMemberSolution::new(
                singleton_inner_puzzle_hash.into_rust()?,
                singleton_amount.into_rust()?,
            ))
            .map_err(js_err)?;

        self.spend.members.insert(
            member_hash,
            MemberSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, member_solution),
            ),
        );

        Ok(())
    }

    #[napi]
    pub fn spend_fixed_puzzle(
        &mut self,
        clvm: &mut ClvmAllocator,
        config: MemberConfig,
        fixed_puzzle_hash: Uint8Array,
    ) -> Result<()> {
        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions)?;

        let member = FixedPuzzleMember::new(fixed_puzzle_hash.into_rust()?);

        let member_hash = member_puzzle_hash(
            nonce,
            restrictions.clone(),
            member.curry_tree_hash(),
            config.top_level,
        );

        let member_puzzle = clvm.0.curry(member).map_err(js_err)?;

        self.spend.members.insert(
            member_hash,
            MemberSpend::new(
                nonce,
                restrictions,
                sdk::Spend::new(member_puzzle, NodePtr::NIL),
            ),
        );

        Ok(())
    }

    #[napi]
    pub fn spend_custom_member(
        &mut self,
        clvm: &mut ClvmAllocator,
        config: MemberConfig,
        spend: Spend,
    ) -> Result<()> {
        let nonce = config.nonce.try_into().unwrap();
        let restrictions = convert_restrictions(config.restrictions)?;

        let member_hash = member_puzzle_hash(
            nonce,
            restrictions.clone(),
            clvm.0.tree_hash(spend.puzzle.ptr),
            config.top_level,
        );

        self.spend.members.insert(
            member_hash,
            MemberSpend::new(nonce, restrictions, spend.into_rust()?),
        );

        Ok(())
    }

    #[napi]
    pub fn spend_recovery_restriction(
        &mut self,
        clvm: &mut ClvmAllocator,
        left_side_subtree_hash: Uint8Array,
        nonce: u32,
        member_validator_list_hash: Uint8Array,
        delegated_puzzle_validator_list_hash: Uint8Array,
        new_right_side_member_hash: Uint8Array,
    ) -> Result<()> {
        let restriction = Recovery::new(
            left_side_subtree_hash.into_rust()?,
            nonce.try_into().unwrap(),
            member_validator_list_hash.into_rust()?,
            delegated_puzzle_validator_list_hash.into_rust()?,
        );

        let puzzle = clvm.0.curry(restriction).map_err(js_err)?;

        let solution = clvm
            .0
            .alloc(&RecoverySolution::new(
                new_right_side_member_hash.into_rust()?,
            ))
            .map_err(js_err)?;

        self.spend.restrictions.insert(
            restriction.curry_tree_hash(),
            sdk::Spend::new(puzzle, solution),
        );

        Ok(())
    }

    #[napi]
    pub fn spend_timelock_restriction(
        &mut self,
        clvm: &mut ClvmAllocator,
        timelock: BigInt,
    ) -> Result<()> {
        let restriction = Timelock::new(timelock.into_rust()?);
        let puzzle = clvm.0.curry(restriction).map_err(js_err)?;
        self.spend.restrictions.insert(
            restriction.curry_tree_hash(),
            sdk::Spend::new(puzzle, NodePtr::NIL),
        );
        Ok(())
    }
}

#[napi(object)]
pub struct Restriction {
    pub is_member_condition_validator: bool,
    pub puzzle_hash: Uint8Array,
}

impl IntoJs<Restriction> for sdk::Restriction {
    fn into_js(self) -> Result<Restriction> {
        Ok(Restriction {
            is_member_condition_validator: self.is_member_condition_validator,
            puzzle_hash: self.puzzle_hash.into_js()?,
        })
    }
}

impl FromJs<Restriction> for sdk::Restriction {
    fn from_js(restriction: Restriction) -> Result<Self> {
        Ok(sdk::Restriction {
            is_member_condition_validator: restriction.is_member_condition_validator,
            puzzle_hash: restriction.puzzle_hash.into_rust()?,
        })
    }
}

fn convert_restrictions(restrictions: Vec<Restriction>) -> Result<Vec<sdk::Restriction>> {
    restrictions
        .into_iter()
        .map(IntoRust::into_rust)
        .collect::<Result<Vec<_>>>()
}

#[napi(object)]
pub struct MemberConfig {
    pub top_level: bool,
    pub nonce: u32,
    pub restrictions: Vec<Restriction>,
}

fn member_hash(config: MemberConfig, inner_hash: TreeHash) -> Result<Uint8Array> {
    member_puzzle_hash(
        config.nonce.try_into().unwrap(),
        convert_restrictions(config.restrictions)?,
        inner_hash,
        config.top_level,
    )
    .into_js()
}

#[napi]
pub fn m_of_n_hash(
    config: MemberConfig,
    required: u32,
    items: Vec<Uint8Array>,
) -> Result<Uint8Array> {
    member_hash(
        config,
        MofN::new(
            required.try_into().unwrap(),
            items
                .into_iter()
                .map(IntoRust::into_rust)
                .collect::<Result<Vec<_>>>()?,
        )
        .inner_puzzle_hash(),
    )
}

#[napi]
pub fn k1_member_hash(
    config: MemberConfig,
    public_key: ClassInstance<K1PublicKey>,
    fast_forward: bool,
) -> Result<Uint8Array> {
    member_hash(
        config,
        if fast_forward {
            Secp256k1MemberPuzzleAssert::new(public_key.0).curry_tree_hash()
        } else {
            Secp256k1Member::new(public_key.0).curry_tree_hash()
        },
    )
}

#[napi]
pub fn r1_member_hash(
    config: MemberConfig,
    public_key: ClassInstance<R1PublicKey>,
    fast_forward: bool,
) -> Result<Uint8Array> {
    member_hash(
        config,
        if fast_forward {
            Secp256r1MemberPuzzleAssert::new(public_key.0).curry_tree_hash()
        } else {
            Secp256r1Member::new(public_key.0).curry_tree_hash()
        },
    )
}

#[napi]
pub fn bls_member_hash(
    config: MemberConfig,
    public_key: ClassInstance<PublicKey>,
) -> Result<Uint8Array> {
    member_hash(config, BlsMember::new(public_key.0).curry_tree_hash())
}

#[napi]
pub fn passkey_member_hash(
    config: MemberConfig,
    genesis_challenge: Uint8Array,
    public_key: ClassInstance<R1PublicKey>,
    fast_forward: bool,
) -> Result<Uint8Array> {
    member_hash(
        config,
        if fast_forward {
            PasskeyMemberPuzzleAssert::new(genesis_challenge.into_rust()?, public_key.0)
                .curry_tree_hash()
        } else {
            PasskeyMember::new(genesis_challenge.into_rust()?, public_key.0).curry_tree_hash()
        },
    )
}

#[napi]
pub fn singleton_member_hash(config: MemberConfig, launcher_id: Uint8Array) -> Result<Uint8Array> {
    member_hash(
        config,
        SingletonMember::new(launcher_id.into_rust()?).curry_tree_hash(),
    )
}

#[napi]
pub fn fixed_member_hash(
    config: MemberConfig,
    fixed_puzzle_hash: Uint8Array,
) -> Result<Uint8Array> {
    member_hash(
        config,
        FixedPuzzleMember::new(fixed_puzzle_hash.into_rust()?).curry_tree_hash(),
    )
}

#[napi]
pub fn custom_member_hash(config: MemberConfig, inner_hash: Uint8Array) -> Result<Uint8Array> {
    member_hash(config, inner_hash.into_rust()?)
}

#[napi]
pub fn recovery_restriction(
    left_side_subtree_hash: Uint8Array,
    nonce: u32,
    member_validator_list_hash: Uint8Array,
    delegated_puzzle_validator_list_hash: Uint8Array,
) -> Result<Restriction> {
    Ok(Restriction {
        is_member_condition_validator: true,
        puzzle_hash: Recovery::new(
            left_side_subtree_hash.into_rust()?,
            nonce.try_into().unwrap(),
            member_validator_list_hash.into_rust()?,
            delegated_puzzle_validator_list_hash.into_rust()?,
        )
        .curry_tree_hash()
        .into_js()?,
    })
}

#[napi]
pub fn timelock_restriction(timelock: BigInt) -> Result<Restriction> {
    Ok(Restriction {
        is_member_condition_validator: true,
        puzzle_hash: Timelock::new(timelock.into_rust()?)
            .curry_tree_hash()
            .into_js()?,
    })
}

#[napi]
pub fn p2_singleton_message_puzzle_hash(launcher_id: Uint8Array) -> Result<Uint8Array> {
    P2SingletonMessageArgs::new(launcher_id.into_rust()?)
        .curry_tree_hash()
        .into_js()
}
