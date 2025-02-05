use chia::{
    bls,
    clvm_traits::{clvm_quote, ClvmEncoder, FromClvm, ToClvm},
    clvm_utils::{self, CurriedProgram, TreeHash},
    protocol::{self, Bytes32},
    puzzles::nft::{self, NFT_METADATA_UPDATER_PUZZLE_HASH},
};
use chia_wallet_sdk::{
    self as sdk, AddDelegatedPuzzleWrapper, AddDelegatedPuzzleWrapperSolution,
    Force1of2RestrictedVariable, Force1of2RestrictedVariableSolution, HashedPtr, Memos,
    PreventConditionOpcode, SpendContext,
};
use clvmr::{
    run_program,
    serde::{node_from_bytes, node_from_bytes_backrefs},
    ChiaDialect, NodePtr, MEMPOOL_MODE,
};
use napi::bindgen_prelude::*;
use paste::paste;

use crate::{
    clvm_value::{Allocate, ClvmValue},
    traits::{js_err, FromJs, IntoJs, IntoJsContextual, IntoRust},
    Coin, CoinSpend, MintedNfts, MipsSpend, Nft, NftMetadata, NftMint, ParsedNft, Program,
    PublicKey, Spend, Vault, VaultMint,
};

pub type Clvm = Reference<ClvmAllocator>;

#[napi]
pub struct ClvmAllocator(pub(crate) SpendContext);

#[napi]
impl ClvmAllocator {
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        Ok(Self(SpendContext::new()))
    }

    #[napi(ts_args_type = "")]
    pub fn nil(&mut self, this: This<Clvm>) -> Result<Program> {
        Ok(Program::new(this, NodePtr::NIL))
    }

    #[napi(ts_args_type = "value: Uint8Array")]
    pub fn deserialize(&mut self, this: This<Clvm>, value: Uint8Array) -> Result<Program> {
        let ptr = node_from_bytes(&mut self.0.allocator, &value)?;
        Ok(Program::new(this, ptr))
    }

    #[napi(ts_args_type = "value: Uint8Array")]
    pub fn deserialize_with_backrefs(
        &mut self,
        this: This<Clvm>,
        value: Uint8Array,
    ) -> Result<Program> {
        let ptr = node_from_bytes_backrefs(&mut self.0.allocator, &value)?;
        Ok(Program::new(this, ptr))
    }

    #[napi]
    pub fn tree_hash(&self, program: &Program) -> Result<Uint8Array> {
        self.0.tree_hash(program.ptr).to_bytes().into_js()
    }

    #[napi(
        ts_args_type = "puzzle: Program, solution: Program, maxCost: bigint, mempoolMode: boolean"
    )]
    pub fn run(
        &mut self,
        env: Env,
        this: This<Clvm>,
        puzzle: &Program,
        solution: &Program,
        max_cost: BigInt,
        mempool_mode: bool,
    ) -> Result<Output> {
        let mut flags = 0;

        if mempool_mode {
            flags |= MEMPOOL_MODE;
        }

        let result = run_program(
            &mut self.0.allocator,
            &ChiaDialect::new(flags),
            puzzle.ptr,
            solution.ptr,
            max_cost.into_rust()?,
        )
        .map_err(js_err)?;

        Ok(Output {
            value: Program::new(this, result.1).into_instance(env)?,
            cost: result.0.into_js()?,
        })
    }

    #[napi(ts_args_type = "program: Program, args: Array<Program>")]
    pub fn curry(
        &mut self,
        this: This<Clvm>,
        program: &Program,
        args: Vec<ClassInstance<Program>>,
    ) -> Result<Program> {
        let mut args_ptr = self.0.allocator.one();

        for arg in args.into_iter().rev() {
            args_ptr = self
                .0
                .allocator
                .encode_curried_arg(arg.ptr, args_ptr)
                .map_err(js_err)?;
        }

        self.0
            .alloc(&CurriedProgram {
                program: program.ptr,
                args: args_ptr,
            })
            .map_err(js_err)
            .map(|ptr| Program::new(this, ptr))
    }

    #[napi(ts_args_type = "first: ClvmValue, rest: ClvmValue")]
    pub fn pair(&mut self, this: This<Clvm>, first: ClvmValue, rest: ClvmValue) -> Result<Program> {
        let first = first.allocate(&mut self.0.allocator)?;
        let rest = rest.allocate(&mut self.0.allocator)?;
        let ptr = self.0.allocator.new_pair(first, rest).map_err(js_err)?;
        Ok(Program::new(this, ptr))
    }

    #[napi(ts_args_type = "value: ClvmValue")]
    pub fn alloc(&mut self, this: This<Clvm>, value: ClvmValue) -> Result<Program> {
        let ptr = value.allocate(&mut self.0.allocator)?;
        Ok(Program::new(this, ptr))
    }

    #[napi]
    pub fn coin_spends(&mut self) -> Result<Vec<CoinSpend>> {
        self.0.take().into_iter().map(IntoJs::into_js).collect()
    }

    #[napi(ts_args_type = "value: NftMetadata")]
    pub fn nft_metadata(&mut self, this: This<Clvm>, value: NftMetadata) -> Result<Program> {
        let metadata = nft::NftMetadata::from_js(value)?;

        let ptr = metadata.to_clvm(&mut self.0.allocator).map_err(js_err)?;

        Ok(Program::new(this, ptr))
    }

    #[napi(ts_args_type = "value: Program")]
    pub fn parse_nft_metadata(&mut self, value: &Program) -> Result<NftMetadata> {
        let metadata = nft::NftMetadata::from_clvm(&self.0.allocator, value.ptr).map_err(js_err)?;

        metadata.into_js()
    }

    #[napi(ts_args_type = "conditions: Array<Program>")]
    pub fn delegated_spend_for_conditions(
        &mut self,
        env: Env,
        this: This<Clvm>,
        conditions: Vec<ClassInstance<Program>>,
    ) -> Result<Spend> {
        let conditions: Vec<NodePtr> = conditions.into_iter().map(|program| program.ptr).collect();

        let delegated_puzzle = self.0.alloc(&clvm_quote!(conditions)).map_err(js_err)?;

        Ok(Spend {
            puzzle: Program::new(this.clone(env)?, delegated_puzzle).into_instance(env)?,
            solution: Program::new(this, NodePtr::NIL).into_instance(env)?,
        })
    }

    #[napi(ts_args_type = "syntheticKey: PublicKey, delegatedSpend: Spend")]
    pub fn spend_p2_standard(
        &mut self,
        env: Env,
        this: This<Clvm>,
        synthetic_key: Reference<PublicKey>,
        delegated_spend: Spend,
    ) -> Result<Spend> {
        let ctx = &mut self.0;
        let synthetic_key = synthetic_key.0;
        let p2 = sdk::StandardLayer::new(synthetic_key);

        let spend = p2
            .delegated_inner_spend(
                ctx,
                sdk::Spend::new(delegated_spend.puzzle.ptr, delegated_spend.solution.ptr),
            )
            .map_err(js_err)?;

        Ok(Spend {
            puzzle: Program::new(this.clone(env)?, spend.puzzle).into_instance(env)?,
            solution: Program::new(this, spend.solution).into_instance(env)?,
        })
    }

    #[napi(ts_args_type = "parent_coin_id: Uint8Array, nft_mints: Array<NftMint>")]
    pub fn mint_nfts(
        &mut self,
        env: Env,
        this: This<Clvm>,
        parent_coin_id: Uint8Array,
        nft_mints: Vec<NftMint>,
    ) -> Result<MintedNfts> {
        let parent_coin_id = parent_coin_id.into_rust()?;

        let mut result = MintedNfts {
            nfts: Vec::new(),
            parent_conditions: Vec::new(),
        };

        for (i, nft_mint) in nft_mints.into_iter().enumerate() {
            let (conditions, nft) = sdk::Launcher::new(parent_coin_id, i as u64 * 2 + 1)
                .mint_nft(
                    &mut self.0,
                    sdk::NftMint::<nft::NftMetadata> {
                        metadata: nft_mint.metadata.into_rust()?,
                        p2_puzzle_hash: nft_mint.p2_puzzle_hash.into_rust()?,
                        royalty_puzzle_hash: nft_mint.royalty_puzzle_hash.into_rust()?,
                        royalty_ten_thousandths: nft_mint.royalty_ten_thousandths,
                        metadata_updater_puzzle_hash: NFT_METADATA_UPDATER_PUZZLE_HASH.into(),
                        owner: None,
                    },
                )
                .map_err(js_err)?;

            let serialized_metadata = self.0.serialize(&nft.info.metadata).map_err(js_err)?;

            result
                .nfts
                .push(nft.with_metadata(serialized_metadata).into_js()?);

            for condition in conditions {
                let condition = condition.to_clvm(&mut self.0.allocator).map_err(js_err)?;

                result
                    .parent_conditions
                    .push(Program::new(this.clone(env)?, condition).into_instance(env)?);
            }
        }

        Ok(result)
    }

    #[napi(ts_args_type = "puzzle: Program")]
    pub fn parse_nft_info(
        &mut self,
        env: Env,
        this: This<Clvm>,
        puzzle: &Program,
    ) -> Result<Option<ParsedNft>> {
        let puzzle = sdk::Puzzle::parse(&self.0.allocator, puzzle.ptr);

        let Some((nft_info, inner_puzzle)) =
            sdk::NftInfo::<protocol::Program>::parse(&self.0.allocator, puzzle).map_err(js_err)?
        else {
            return Ok(None);
        };

        Ok(Some(ParsedNft {
            info: nft_info.into_js()?,
            inner_puzzle: Program::new(this, inner_puzzle.ptr()).into_instance(env)?,
        }))
    }

    #[napi]
    pub fn parse_child_nft(
        &mut self,
        parent_coin: Coin,
        parent_puzzle: &Program,
        parent_solution: &Program,
    ) -> Result<Option<Nft>> {
        let parent_puzzle = sdk::Puzzle::parse(&self.0.allocator, parent_puzzle.ptr);

        let Some(nft) = sdk::Nft::<HashedPtr>::parse_child(
            &mut self.0.allocator,
            parent_coin.into_rust()?,
            parent_puzzle,
            parent_solution.ptr,
        )
        .map_err(js_err)?
        else {
            return Ok(None);
        };

        let serialized_metadata = self.0.serialize(&nft.info.metadata).map_err(js_err)?;

        Ok(Some(nft.with_metadata(serialized_metadata).into_js()?))
    }

    #[napi]
    pub fn spend_nft(&mut self, nft: Nft, inner_spend: Spend) -> Result<()> {
        let ctx = &mut self.0;
        let nft = sdk::Nft::<protocol::Program>::from_js(nft)?;

        nft.spend(
            ctx,
            sdk::Spend::new(inner_spend.puzzle.ptr, inner_spend.solution.ptr),
        )
        .map_err(js_err)?;

        Ok(())
    }

    #[napi(ts_args_type = "parentCoinId: Uint8Array, custodyHash: Uint8Array, memos: Program")]
    pub fn mint_vault(
        &mut self,
        env: Env,
        this: This<Clvm>,
        parent_coin_id: Uint8Array,
        custody_hash: Uint8Array,
        memos: &Program,
    ) -> Result<VaultMint> {
        let (parent_conditions, vault) = sdk::Launcher::new(parent_coin_id.into_rust()?, 1)
            .mint_vault(&mut self.0, custody_hash.into_rust()?, memos.ptr)
            .map_err(js_err)?;

        let parent_conditions: Vec<ClassInstance<Program>> = parent_conditions
            .into_iter()
            .map(|program| {
                Program::new(
                    this.clone(env)?,
                    program.to_clvm(&mut self.0.allocator).map_err(js_err)?,
                )
                .into_instance(env)
            })
            .collect::<Result<Vec<_>>>()?;

        Ok(VaultMint {
            parent_conditions,
            vault: vault.into_js()?,
        })
    }

    #[napi]
    pub fn spend_vault(&mut self, vault: Vault, spend: &MipsSpend) -> Result<()> {
        let rust: sdk::Vault = vault.into_rust()?;
        rust.spend(&mut self.0, &spend.spend).map_err(js_err)?;
        Ok(())
    }

    #[napi(
        ts_args_type = "spend: Spend, leftSideSubtreeHash: Uint8Array, nonce: number, memberValidatorListHash: Uint8Array, delegatedPuzzleValidatorListHash: Uint8Array, newRightSideMemberHash: Uint8Array"
    )]
    pub fn wrap_with_force_1_of_2(
        &mut self,
        env: Env,
        this: This<Clvm>,
        spend: Spend,
        left_side_subtree_hash: Uint8Array,
        nonce: u32,
        member_validator_list_hash: Uint8Array,
        delegated_puzzle_validator_list_hash: Uint8Array,
        new_right_side_member_hash: Uint8Array,
    ) -> Result<Spend> {
        let wrapper = Force1of2RestrictedVariable::new(
            left_side_subtree_hash.into_rust()?,
            nonce.try_into().unwrap(),
            member_validator_list_hash.into_rust()?,
            delegated_puzzle_validator_list_hash.into_rust()?,
        );

        let puzzle = self.0.curry(wrapper).map_err(js_err)?;

        let solution = self
            .0
            .alloc(&Force1of2RestrictedVariableSolution::new(
                new_right_side_member_hash.into_rust()?,
            ))
            .map_err(js_err)?;

        let puzzle = self
            .0
            .curry(AddDelegatedPuzzleWrapper::new(puzzle, spend.puzzle.ptr))
            .map_err(js_err)?;

        let solution = self
            .0
            .alloc(&AddDelegatedPuzzleWrapperSolution::new(
                solution,
                spend.solution.ptr,
            ))
            .map_err(js_err)?;

        Ok(Spend {
            puzzle: Program::new(this.clone(env)?, puzzle).into_instance(env)?,
            solution: Program::new(this, solution).into_instance(env)?,
        })
    }

    #[napi(ts_args_type = "spend: Spend, conditionOpcode: number")]
    pub fn wrap_with_prevent_condition_opcode(
        &mut self,
        env: Env,
        this: This<Clvm>,
        spend: Spend,
        condition_opcode: u16,
    ) -> Result<Spend> {
        let wrapper = PreventConditionOpcode::new(condition_opcode);

        let puzzle = self.0.curry(wrapper).map_err(js_err)?;
        let solution = NodePtr::NIL;

        let puzzle = self
            .0
            .curry(AddDelegatedPuzzleWrapper::new(puzzle, spend.puzzle.ptr))
            .map_err(js_err)?;

        let solution = self
            .0
            .alloc(&AddDelegatedPuzzleWrapperSolution::new(
                solution,
                spend.solution.ptr,
            ))
            .map_err(js_err)?;

        Ok(Spend {
            puzzle: Program::new(this.clone(env)?, puzzle).into_instance(env)?,
            solution: Program::new(this, solution).into_instance(env)?,
        })
    }

    #[napi(ts_args_type = "spend: Spend")]
    pub fn wrap_with_prevent_multiple_create_coins(
        &mut self,
        env: Env,
        this: This<Clvm>,
        spend: Spend,
    ) -> Result<Spend> {
        let puzzle = self
            .0
            .prevent_multiple_create_coins_puzzle()
            .map_err(js_err)?;
        let solution = NodePtr::NIL;

        let puzzle = self
            .0
            .curry(AddDelegatedPuzzleWrapper::new(puzzle, spend.puzzle.ptr))
            .map_err(js_err)?;

        let solution = self
            .0
            .alloc(&AddDelegatedPuzzleWrapperSolution::new(
                solution,
                spend.solution.ptr,
            ))
            .map_err(js_err)?;

        Ok(Spend {
            puzzle: Program::new(this.clone(env)?, puzzle).into_instance(env)?,
            solution: Program::new(this, solution).into_instance(env)?,
        })
    }
}

#[napi(object)]
pub struct Output {
    pub value: ClassInstance<Program>,
    pub cost: BigInt,
}

#[napi]
pub fn curry_tree_hash(tree_hash: Uint8Array, args: Vec<Uint8Array>) -> Result<Uint8Array> {
    let tree_hash: Bytes32 = tree_hash.into_rust()?;
    let args: Vec<TreeHash> = args
        .into_iter()
        .map(|arg| Ok(TreeHash::new(arg.into_rust()?)))
        .collect::<Result<Vec<_>>>()?;
    clvm_utils::curry_tree_hash(tree_hash.into(), &args)
        .to_bytes()
        .into_js()
}

#[napi]
pub fn int_to_signed_bytes(big_int: BigInt) -> Result<Uint8Array> {
    let number: num_bigint::BigInt = big_int.into_rust()?;
    number.to_signed_bytes_be().into_js()
}

#[napi]
pub fn signed_bytes_to_int(bytes: Uint8Array) -> Result<BigInt> {
    let bytes: Vec<u8> = bytes.into_rust()?;
    let number = num_bigint::BigInt::from_signed_bytes_be(&bytes);
    number.into_js()
}

macro_rules! conditions {
    ( $( $condition:ident $( < $( $generic:ty ),* > )? { $hint:literal $function:ident( $( $name:ident: $ty:ty $( => $remap:ty )? ),* ) }, )* ) => {
        $( #[napi(object)]
        pub struct $condition {
            $( pub $name: $ty, )*
        } )*

        $( paste! {
            #[napi]
            impl ClvmAllocator {
                #[napi(ts_args_type = $hint)]
                pub fn $function( &mut self, this: This<Clvm>, $( $name: $ty ),* ) -> Result<Program> {
                    $( let $name $( : $remap )? = FromJs::from_js($name)?; )*
                    let ptr = sdk::$condition::new( $( $name ),* )
                    .to_clvm(&mut self.0.allocator)
                    .map_err(js_err)?;

                    Ok(Program::new(this, ptr))
                }

                #[napi(ts_args_type = "program: Program")]
                #[allow(unused)]
                pub fn [< parse_ $function >]( &mut self, env: Env, this: This<Clvm>, program: Reference<Program> ) -> Result<Option<$condition>> {
                    let Some(condition) = sdk::$condition $( ::< $( $generic ),* > )? ::from_clvm(&self.0.allocator, program.ptr).ok() else {
                        return Ok(None);
                    };

                    Ok(Some($condition {
                        $( $name: condition.$name.into_js_contextual(env, this.clone(env)?, self)?, )*
                    }))
                }
            }
        } )*
    };
}

conditions!(
    Remark<NodePtr> {
        "rest: Program"
        remark(rest: ClassInstance<Program> => NodePtr)
    },
    AggSigParent {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_parent(public_key: ClassInstance<PublicKey> => bls::PublicKey, message: Uint8Array)
    },
    AggSigPuzzle {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_puzzle(public_key: ClassInstance<PublicKey> => bls::PublicKey, message: Uint8Array)
    },
    AggSigAmount {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_amount(public_key: ClassInstance<PublicKey> => bls::PublicKey, message: Uint8Array)
    },
    AggSigPuzzleAmount {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_puzzle_amount(public_key: ClassInstance<PublicKey> => bls::PublicKey, message: Uint8Array)
    },
    AggSigParentAmount {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_parent_amount(public_key: ClassInstance<PublicKey> => bls::PublicKey, message: Uint8Array)
    },
    AggSigParentPuzzle {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_parent_puzzle(public_key: ClassInstance<PublicKey> => bls::PublicKey, message: Uint8Array)
    },
    AggSigUnsafe {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_unsafe(public_key: ClassInstance<PublicKey> => bls::PublicKey, message: Uint8Array)
    },
    AggSigMe {
        "publicKey: PublicKey, message: Uint8Array"
        agg_sig_me(public_key: ClassInstance<PublicKey> => bls::PublicKey, message: Uint8Array)
    },
    CreateCoin {
        "puzzleHash: Uint8Array, amount: bigint, memos: Program | null"
        create_coin(puzzle_hash: Uint8Array, amount: BigInt, memos: Option<ClassInstance<Program>> => Option<Memos<NodePtr>>)
    },
    ReserveFee {
        "amount: bigint"
        reserve_fee(amount: BigInt)
    },
    CreateCoinAnnouncement {
        "message: Uint8Array"
        create_coin_announcement(message: Uint8Array)
    },
    CreatePuzzleAnnouncement {
        "message: Uint8Array"
        create_puzzle_announcement(message: Uint8Array)
    },
    AssertCoinAnnouncement {
        "announcementId: Uint8Array"
        assert_coin_announcement(announcement_id: Uint8Array)
    },
    AssertPuzzleAnnouncement {
        "announcementId: Uint8Array"
        assert_puzzle_announcement(announcement_id: Uint8Array)
    },
    AssertConcurrentSpend {
        "coinId: Uint8Array"
        assert_concurrent_spend(coin_id: Uint8Array)
    },
    AssertConcurrentPuzzle {
        "puzzleHash: Uint8Array"
        assert_concurrent_puzzle(puzzle_hash: Uint8Array)
    },
    AssertSecondsRelative {
        "seconds: bigint"
        assert_seconds_relative(seconds: BigInt)
    },
    AssertSecondsAbsolute {
        "seconds: bigint"
        assert_seconds_absolute(seconds: BigInt)
    },
    AssertHeightRelative {
        "height: number"
        assert_height_relative(height: u32)
    },
    AssertHeightAbsolute {
        "height: number"
        assert_height_absolute(height: u32)
    },
    AssertBeforeSecondsRelative {
        "seconds: bigint"
        assert_before_seconds_relative(seconds: BigInt)
    },
    AssertBeforeSecondsAbsolute {
        "seconds: bigint"
        assert_before_seconds_absolute(seconds: BigInt)
    },
    AssertBeforeHeightRelative {
        "height: number"
        assert_before_height_relative(height: u32)
    },
    AssertBeforeHeightAbsolute {
        "height: number"
        assert_before_height_absolute(height: u32)
    },
    AssertMyCoinId {
        "coinId: Uint8Array"
        assert_my_coin_id(coin_id: Uint8Array)
    },
    AssertMyParentId {
        "parentId: Uint8Array"
        assert_my_parent_id(parent_id: Uint8Array)
    },
    AssertMyPuzzleHash {
        "puzzleHash: Uint8Array"
        assert_my_puzzle_hash(puzzle_hash: Uint8Array)
    },
    AssertMyAmount {
        "amount: bigint"
        assert_my_amount(amount: BigInt)
    },
    AssertMyBirthSeconds {
        "seconds: bigint"
        assert_my_birth_seconds(seconds: BigInt)
    },
    AssertMyBirthHeight {
        "height: number"
        assert_my_birth_height(height: u32)
    },
    AssertEphemeral {
        ""
        assert_ephemeral()
    },
    SendMessage<NodePtr> {
        "mode: number, message: Uint8Array, data: Array<Program>"
        send_message(mode: u8, message: Uint8Array, data: Vec<ClassInstance<Program>> => Vec<NodePtr>)
    },
    ReceiveMessage<NodePtr> {
        "mode: number, message: Uint8Array, data: Array<Program>"
        receive_message(mode: u8, message: Uint8Array, data: Vec<ClassInstance<Program>> => Vec<NodePtr>)
    },
    Softfork<NodePtr> {
        "cost: bigint, rest: Program"
        softfork(cost: BigInt, rest: ClassInstance<Program> => NodePtr)
    },
);
