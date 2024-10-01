use chia::{
    bls::PublicKey,
    clvm_traits::{clvm_quote, ClvmEncoder, ToClvm},
    clvm_utils::{self, CurriedProgram, TreeHash},
    protocol::Bytes32,
    puzzles::nft::{self, NFT_METADATA_UPDATER_PUZZLE_HASH},
};
use chia_wallet_sdk::{
    self as sdk, AggSigAmount, AggSigMe, AggSigParent, AggSigParentAmount, AggSigParentPuzzle,
    AggSigPuzzle, AggSigPuzzleAmount, AggSigUnsafe, AssertBeforeHeightAbsolute,
    AssertBeforeHeightRelative, AssertBeforeSecondsAbsolute, AssertBeforeSecondsRelative,
    AssertCoinAnnouncement, AssertConcurrentPuzzle, AssertConcurrentSpend, AssertEphemeral,
    AssertHeightAbsolute, AssertHeightRelative, AssertMyAmount, AssertMyBirthHeight,
    AssertMyBirthSeconds, AssertMyCoinId, AssertMyParentId, AssertMyPuzzleHash,
    AssertPuzzleAnnouncement, AssertSecondsAbsolute, AssertSecondsRelative, CreateCoin,
    CreateCoinAnnouncement, CreatePuzzleAnnouncement, Primitive, ReceiveMessage, Remark,
    ReserveFee, SendMessage, Softfork, SpendContext,
};
use clvmr::{
    run_program,
    serde::{node_from_bytes, node_from_bytes_backrefs},
    ChiaDialect, NodePtr, ENABLE_BLS_OPS_OUTSIDE_GUARD, ENABLE_FIXED_DIV, MEMPOOL_MODE,
};
use napi::bindgen_prelude::*;

use crate::{
    clvm_value::{Allocate, ClvmValue},
    traits::{FromJs, IntoJs, IntoRust},
    Coin, CoinSpend, MintedNfts, Nft, NftMint, ParsedNft, Program, Spend,
};

type Clvm = Reference<ClvmAllocator>;

#[napi]
pub struct ClvmAllocator(pub(crate) SpendContext);

macro_rules! conditions {
    ( $( $condition:ident { $hint:literal $function:ident( $( $name:ident: $ty:ty $( => $remap:ty )? ),* ) }, )* ) => {
        $( #[napi]
        impl ClvmAllocator {
            #[napi(ts_args_type = $hint)]
            pub fn $function( &mut self, this: This<Clvm>, $( $name: $ty ),* ) -> Result<Program> {
                $( let $name $( : $remap )? = FromJs::from_js($name)?; )*
                let ptr = $condition::new( $( $name ),* )
                .to_clvm(&mut self.0.allocator)
                .map_err(|error| Error::from_reason(error.to_string()))?;

                Ok(Program { ctx: this, ptr })
            }
        } )*
    };
}

conditions!(
    Remark {
        "value: Program"
        remark(value: ClassInstance<Program> => NodePtr)
    },
    AggSigParent {
        "publicKey: Uint8Array, message: Uint8Array"
        agg_sig_parent(public_key: Uint8Array, message: Uint8Array)
    },
    AggSigPuzzle {
        "publicKey: Uint8Array, message: Uint8Array"
        agg_sig_puzzle(public_key: Uint8Array, message: Uint8Array)
    },
    AggSigAmount {
        "publicKey: Uint8Array, message: Uint8Array"
        agg_sig_amount(public_key: Uint8Array, message: Uint8Array)
    },
    AggSigPuzzleAmount {
        "publicKey: Uint8Array, message: Uint8Array"
        agg_sig_puzzle_amount(public_key: Uint8Array, message: Uint8Array)
    },
    AggSigParentAmount {
        "publicKey: Uint8Array, message: Uint8Array"
        agg_sig_parent_amount(public_key: Uint8Array, message: Uint8Array)
    },
    AggSigParentPuzzle {
        "publicKey: Uint8Array, message: Uint8Array"
        agg_sig_parent_puzzle(public_key: Uint8Array, message: Uint8Array)
    },
    AggSigUnsafe {
        "publicKey: Uint8Array, message: Uint8Array"
        agg_sig_unsafe(public_key: Uint8Array, message: Uint8Array)
    },
    AggSigMe {
        "publicKey: Uint8Array, message: Uint8Array"
        agg_sig_me(public_key: Uint8Array, message: Uint8Array)
    },
    CreateCoin {
        "puzzleHash: Uint8Array, amount: bigint, memos: Array<Uint8Array>"
        create_coin(puzzle_hash: Uint8Array, amount: BigInt, memos: Vec<Uint8Array>)
    },
    ReserveFee {
        "fee: bigint"
        reserve_fee(fee: BigInt)
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
    SendMessage {
        "mode: number, message: Uint8Array, data: Array<Program>"
        send_message(mode: u8, message: Uint8Array, data: Vec<ClassInstance<Program>> => Vec<NodePtr>)
    },
    ReceiveMessage {
        "mode: number, message: Uint8Array, data: Array<Program>"
        receive_message(mode: u8, message: Uint8Array, data: Vec<ClassInstance<Program>> => Vec<NodePtr>)
    },
    Softfork {
        "cost: bigint, value: Program"
        softfork(cost: BigInt, value: ClassInstance<Program> => NodePtr)
    },
);

#[napi]
impl ClvmAllocator {
    #[napi(constructor)]
    pub fn new() -> Result<Self> {
        Ok(Self(SpendContext::new()))
    }

    #[napi(ts_args_type = "")]
    pub fn nil(&mut self, this: This<Clvm>) -> Result<Program> {
        Ok(Program {
            ctx: this,
            ptr: NodePtr::NIL,
        })
    }

    #[napi(ts_args_type = "value: Uint8Array")]
    pub fn deserialize(&mut self, this: This<Clvm>, value: Uint8Array) -> Result<Program> {
        let ptr = node_from_bytes(&mut self.0.allocator, &value)?;
        Ok(Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "value: Uint8Array")]
    pub fn deserialize_with_backrefs(
        &mut self,
        this: This<Clvm>,
        value: Uint8Array,
    ) -> Result<Program> {
        let ptr = node_from_bytes_backrefs(&mut self.0.allocator, &value)?;
        Ok(Program { ctx: this, ptr })
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
        let mut flags = ENABLE_BLS_OPS_OUTSIDE_GUARD | ENABLE_FIXED_DIV;

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
        .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Output {
            value: Program {
                ctx: this,
                ptr: result.1,
            }
            .into_instance(env)?,
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
                .map_err(|error| Error::from_reason(error.to_string()))?;
        }

        self.0
            .alloc(&CurriedProgram {
                program: program.ptr,
                args: args_ptr,
            })
            .map_err(|error| Error::from_reason(error.to_string()))
            .map(|ptr| Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "first: Program, rest: Program")]
    pub fn pair(&mut self, this: This<Clvm>, first: &Program, rest: &Program) -> Result<Program> {
        let ptr = self
            .0
            .allocator
            .new_pair(first.ptr, rest.ptr)
            .map_err(|error| Error::from_reason(error.to_string()))?;
        Ok(Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "value: ClvmValue")]
    pub fn alloc(&mut self, this: This<Clvm>, value: ClvmValue) -> Result<Program> {
        let ptr = value.allocate(&mut self.0.allocator)?;
        Ok(Program { ctx: this, ptr })
    }

    #[napi(ts_args_type = "conditions: Array<Program>")]
    pub fn delegated_spend_for_conditions(
        &mut self,
        env: Env,
        this: This<Clvm>,
        conditions: Vec<ClassInstance<Program>>,
    ) -> Result<Spend> {
        let conditions: Vec<NodePtr> = conditions.into_iter().map(|program| program.ptr).collect();

        let delegated_puzzle = self
            .0
            .alloc(&clvm_quote!(conditions))
            .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Spend {
            puzzle: Program {
                ctx: this.clone(env)?,
                ptr: delegated_puzzle,
            }
            .into_instance(env)?,
            solution: Program {
                ctx: this,
                ptr: NodePtr::NIL,
            }
            .into_instance(env)?,
        })
    }

    #[napi(ts_args_type = "syntheticKey: Uint8Array, delegatedSpend: Spend")]
    pub fn spend_p2_standard(
        &mut self,
        env: Env,
        this: This<Clvm>,
        synthetic_key: Uint8Array,
        delegated_spend: Spend,
    ) -> Result<Spend> {
        let ctx = &mut self.0;
        let synthetic_key = PublicKey::from_js(synthetic_key)?;
        let p2 = sdk::StandardLayer::new(synthetic_key);

        let spend = p2
            .delegated_inner_spend(
                ctx,
                sdk::Spend {
                    puzzle: delegated_spend.puzzle.ptr,
                    solution: delegated_spend.solution.ptr,
                },
            )
            .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Spend {
            puzzle: Program {
                ctx: this.clone(env)?,
                ptr: spend.puzzle,
            }
            .into_instance(env)?,
            solution: Program {
                ctx: this,
                ptr: spend.solution,
            }
            .into_instance(env)?,
        })
    }

    #[napi(
        ts_args_type = "launcherId: Uint8Array, coinId: Uint8Array, singletonInnerPuzzleHash: Uint8Array, delegatedSpend: Spend"
    )]
    pub fn spend_p2_delegated_singleton(
        &mut self,
        env: Env,
        this: This<Clvm>,
        launcher_id: Uint8Array,
        coin_id: Uint8Array,
        singleton_inner_puzzle_hash: Uint8Array,
        delegated_spend: Spend,
    ) -> Result<Spend> {
        let p2 = sdk::P2DelegatedSingletonLayer::new(launcher_id.into_rust()?);

        let spend = p2
            .spend(
                &mut self.0,
                coin_id.into_rust()?,
                singleton_inner_puzzle_hash.into_rust()?,
                sdk::Spend {
                    puzzle: delegated_spend.puzzle.ptr,
                    solution: delegated_spend.solution.ptr,
                },
            )
            .map_err(|error| Error::from_reason(error.to_string()))?;

        Ok(Spend {
            puzzle: Program {
                ctx: this.clone(env)?,
                ptr: spend.puzzle,
            }
            .into_instance(env)?,
            solution: Program {
                ctx: this,
                ptr: spend.solution,
            }
            .into_instance(env)?,
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
            coin_spends: Vec::new(),
            parent_conditions: Vec::new(),
        };

        let len = nft_mints.len();

        for (i, nft_mint) in nft_mints.into_iter().enumerate() {
            let (conditions, nft) = sdk::IntermediateLauncher::new(parent_coin_id, i, len)
                .create(&mut self.0)
                .map_err(|error| Error::from_reason(error.to_string()))?
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
                .map_err(|error| Error::from_reason(error.to_string()))?;

            result.nfts.push(nft.into_js()?);

            for condition in conditions {
                let condition = condition
                    .to_clvm(&mut self.0.allocator)
                    .map_err(|error| Error::from_reason(error.to_string()))?;

                result.parent_conditions.push(
                    Program {
                        ctx: this.clone(env)?,
                        ptr: condition,
                    }
                    .into_instance(env)?,
                );
            }
        }

        result.coin_spends.extend(
            self.0
                .take()
                .into_iter()
                .map(IntoJs::into_js)
                .collect::<Result<Vec<_>>>()?,
        );

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
            sdk::NftInfo::<nft::NftMetadata>::parse(&self.0.allocator, puzzle)
                .map_err(|error| Error::from_reason(error.to_string()))?
        else {
            return Ok(None);
        };

        Ok(Some(ParsedNft {
            info: nft_info.into_js()?,
            inner_puzzle: Program {
                ctx: this,
                ptr: inner_puzzle.ptr(),
            }
            .into_instance(env)?,
        }))
    }

    #[napi]
    pub fn parse_unspent_nft(
        &mut self,
        parent_coin: Coin,
        parent_puzzle: &Program,
        parent_solution: &Program,
        coin: Coin,
    ) -> Result<Option<Nft>> {
        let parent_puzzle = sdk::Puzzle::parse(&self.0.allocator, parent_puzzle.ptr);

        let Some(nft) = sdk::Nft::<nft::NftMetadata>::from_parent_spend(
            &mut self.0.allocator,
            parent_coin.into_rust()?,
            parent_puzzle,
            parent_solution.ptr,
            coin.into_rust()?,
        )
        .map_err(|error| Error::from_reason(error.to_string()))?
        else {
            return Ok(None);
        };

        Ok(Some(nft.into_js()?))
    }

    #[napi]
    pub fn spend_nft(&mut self, nft: Nft, inner_spend: Spend) -> Result<Vec<CoinSpend>> {
        let ctx = &mut self.0;
        let nft = sdk::Nft::<nft::NftMetadata>::from_js(nft)?;

        nft.spend(
            ctx,
            sdk::Spend {
                puzzle: inner_spend.puzzle.ptr,
                solution: inner_spend.solution.ptr,
            },
        )
        .map_err(|error| Error::from_reason(error.to_string()))?;

        ctx.take().into_iter().map(IntoJs::into_js).collect()
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