/* tslint:disable */
/* eslint-disable */
/* prettier-ignore */

/* auto-generated by NAPI-RS */

const { existsSync, readFileSync } = require('fs')
const { join } = require('path')

const { platform, arch } = process

let nativeBinding = null
let localFileExisted = false
let loadError = null

function isMusl() {
  // For Node 10
  if (!process.report || typeof process.report.getReport !== 'function') {
    try {
      const lddPath = require('child_process').execSync('which ldd').toString().trim()
      return readFileSync(lddPath, 'utf8').includes('musl')
    } catch (e) {
      return true
    }
  } else {
    const { glibcVersionRuntime } = process.report.getReport().header
    return !glibcVersionRuntime
  }
}

switch (platform) {
  case 'android':
    switch (arch) {
      case 'arm64':
        localFileExisted = existsSync(join(__dirname, 'chia-wallet-sdk.android-arm64.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./chia-wallet-sdk.android-arm64.node')
          } else {
            nativeBinding = require('chia-wallet-sdk-android-arm64')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'arm':
        localFileExisted = existsSync(join(__dirname, 'chia-wallet-sdk.android-arm-eabi.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./chia-wallet-sdk.android-arm-eabi.node')
          } else {
            nativeBinding = require('chia-wallet-sdk-android-arm-eabi')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on Android ${arch}`)
    }
    break
  case 'win32':
    switch (arch) {
      case 'x64':
        localFileExisted = existsSync(
          join(__dirname, 'chia-wallet-sdk.win32-x64-msvc.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./chia-wallet-sdk.win32-x64-msvc.node')
          } else {
            nativeBinding = require('chia-wallet-sdk-win32-x64-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'ia32':
        localFileExisted = existsSync(
          join(__dirname, 'chia-wallet-sdk.win32-ia32-msvc.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./chia-wallet-sdk.win32-ia32-msvc.node')
          } else {
            nativeBinding = require('chia-wallet-sdk-win32-ia32-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'arm64':
        localFileExisted = existsSync(
          join(__dirname, 'chia-wallet-sdk.win32-arm64-msvc.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./chia-wallet-sdk.win32-arm64-msvc.node')
          } else {
            nativeBinding = require('chia-wallet-sdk-win32-arm64-msvc')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on Windows: ${arch}`)
    }
    break
  case 'darwin':
    localFileExisted = existsSync(join(__dirname, 'chia-wallet-sdk.darwin-universal.node'))
    try {
      if (localFileExisted) {
        nativeBinding = require('./chia-wallet-sdk.darwin-universal.node')
      } else {
        nativeBinding = require('chia-wallet-sdk-darwin-universal')
      }
      break
    } catch {}
    switch (arch) {
      case 'x64':
        localFileExisted = existsSync(join(__dirname, 'chia-wallet-sdk.darwin-x64.node'))
        try {
          if (localFileExisted) {
            nativeBinding = require('./chia-wallet-sdk.darwin-x64.node')
          } else {
            nativeBinding = require('chia-wallet-sdk-darwin-x64')
          }
        } catch (e) {
          loadError = e
        }
        break
      case 'arm64':
        localFileExisted = existsSync(
          join(__dirname, 'chia-wallet-sdk.darwin-arm64.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./chia-wallet-sdk.darwin-arm64.node')
          } else {
            nativeBinding = require('chia-wallet-sdk-darwin-arm64')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on macOS: ${arch}`)
    }
    break
  case 'freebsd':
    if (arch !== 'x64') {
      throw new Error(`Unsupported architecture on FreeBSD: ${arch}`)
    }
    localFileExisted = existsSync(join(__dirname, 'chia-wallet-sdk.freebsd-x64.node'))
    try {
      if (localFileExisted) {
        nativeBinding = require('./chia-wallet-sdk.freebsd-x64.node')
      } else {
        nativeBinding = require('chia-wallet-sdk-freebsd-x64')
      }
    } catch (e) {
      loadError = e
    }
    break
  case 'linux':
    switch (arch) {
      case 'x64':
        if (isMusl()) {
          localFileExisted = existsSync(
            join(__dirname, 'chia-wallet-sdk.linux-x64-musl.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./chia-wallet-sdk.linux-x64-musl.node')
            } else {
              nativeBinding = require('chia-wallet-sdk-linux-x64-musl')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(
            join(__dirname, 'chia-wallet-sdk.linux-x64-gnu.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./chia-wallet-sdk.linux-x64-gnu.node')
            } else {
              nativeBinding = require('chia-wallet-sdk-linux-x64-gnu')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 'arm64':
        if (isMusl()) {
          localFileExisted = existsSync(
            join(__dirname, 'chia-wallet-sdk.linux-arm64-musl.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./chia-wallet-sdk.linux-arm64-musl.node')
            } else {
              nativeBinding = require('chia-wallet-sdk-linux-arm64-musl')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(
            join(__dirname, 'chia-wallet-sdk.linux-arm64-gnu.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./chia-wallet-sdk.linux-arm64-gnu.node')
            } else {
              nativeBinding = require('chia-wallet-sdk-linux-arm64-gnu')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 'arm':
        if (isMusl()) {
          localFileExisted = existsSync(
            join(__dirname, 'chia-wallet-sdk.linux-arm-musleabihf.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./chia-wallet-sdk.linux-arm-musleabihf.node')
            } else {
              nativeBinding = require('chia-wallet-sdk-linux-arm-musleabihf')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(
            join(__dirname, 'chia-wallet-sdk.linux-arm-gnueabihf.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./chia-wallet-sdk.linux-arm-gnueabihf.node')
            } else {
              nativeBinding = require('chia-wallet-sdk-linux-arm-gnueabihf')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 'riscv64':
        if (isMusl()) {
          localFileExisted = existsSync(
            join(__dirname, 'chia-wallet-sdk.linux-riscv64-musl.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./chia-wallet-sdk.linux-riscv64-musl.node')
            } else {
              nativeBinding = require('chia-wallet-sdk-linux-riscv64-musl')
            }
          } catch (e) {
            loadError = e
          }
        } else {
          localFileExisted = existsSync(
            join(__dirname, 'chia-wallet-sdk.linux-riscv64-gnu.node')
          )
          try {
            if (localFileExisted) {
              nativeBinding = require('./chia-wallet-sdk.linux-riscv64-gnu.node')
            } else {
              nativeBinding = require('chia-wallet-sdk-linux-riscv64-gnu')
            }
          } catch (e) {
            loadError = e
          }
        }
        break
      case 's390x':
        localFileExisted = existsSync(
          join(__dirname, 'chia-wallet-sdk.linux-s390x-gnu.node')
        )
        try {
          if (localFileExisted) {
            nativeBinding = require('./chia-wallet-sdk.linux-s390x-gnu.node')
          } else {
            nativeBinding = require('chia-wallet-sdk-linux-s390x-gnu')
          }
        } catch (e) {
          loadError = e
        }
        break
      default:
        throw new Error(`Unsupported architecture on Linux: ${arch}`)
    }
    break
  default:
    throw new Error(`Unsupported OS: ${platform}, architecture: ${arch}`)
}

if (!nativeBinding) {
  if (loadError) {
    throw loadError
  }
  throw new Error(`Failed to load native binding`)
}

const { Puzzle, StreamedCatParsingResult, Cat, CatSpend, ParsedCat, Nft, NftInfo, ParsedNft, NftMetadata, NftMint, DidOwner, MintedNfts, Did, DidInfo, ParsedDid, standardPuzzleHash, catPuzzleHash, StreamingPuzzleInfo, StreamedCat, Output, Pair, CurriedProgram, LineageProof, SecretKey, PublicKey, Signature, Program, Constants, fromHex, toHex, bytesEqual, treeHashAtom, treeHashPair, sha256, curryTreeHash, generateBytes, Simulator, BlsPair, BlsPairWithCoin, K1Pair, R1Pair, Vault, MemberConfig, mOfNHash, k1MemberHash, r1MemberHash, blsMemberHash, passkeyMemberHash, singletonMemberHash, fixedMemberHash, customMemberHash, Restriction, RestrictionKind, timelockRestriction, force1Of2Restriction, preventConditionOpcodeRestriction, preventMultipleCreateCoinsRestriction, preventSideEffectsRestriction, MipsSpend, VaultMint, wrappedDelegatedPuzzleHash, Coin, CoinSpend, SpendBundle, Spend, CoinsetClient, BlockchainStateResponse, BlockchainState, MempoolMinFees, SyncState, AdditionsAndRemovalsResponse, GetBlockResponse, GetBlockRecordResponse, GetBlockRecordsResponse, GetBlocksResponse, GetBlockSpendsResponse, GetCoinRecordResponse, GetCoinRecordsResponse, GetPuzzleAndSolutionResponse, PushTxResponse, GetNetworkInfoResponse, GetMempoolItemResponse, GetMempoolItemsResponse, CoinRecord, MempoolItem, FullBlock, EndOfSubSlotBundle, ChallengeChainSubSlot, InfusedChallengeChainSubSlot, RewardChainSubSlot, SubSlotProofs, VdfInfo, VdfProof, TransactionsInfo, RewardChainBlock, FoliageTransactionBlock, FoliageBlockData, Foliage, PoolTarget, BlockRecord, ProofOfSpace, SubEpochSummary, Address, K1SecretKey, K1PublicKey, K1Signature, R1SecretKey, R1PublicKey, R1Signature, Clvm, Remark, AggSigParent, AggSigPuzzle, AggSigAmount, AggSigPuzzleAmount, AggSigParentAmount, AggSigParentPuzzle, AggSigUnsafe, AggSigMe, CreateCoin, ReserveFee, CreateCoinAnnouncement, CreatePuzzleAnnouncement, AssertCoinAnnouncement, AssertPuzzleAnnouncement, AssertConcurrentSpend, AssertConcurrentPuzzle, AssertSecondsRelative, AssertSecondsAbsolute, AssertHeightRelative, AssertHeightAbsolute, AssertBeforeSecondsRelative, AssertBeforeSecondsAbsolute, AssertBeforeHeightRelative, AssertBeforeHeightAbsolute, AssertMyCoinId, AssertMyParentId, AssertMyPuzzleHash, AssertMyAmount, AssertMyBirthSeconds, AssertMyBirthHeight, AssertEphemeral, SendMessage, ReceiveMessage, Softfork, Mnemonic } = nativeBinding

module.exports.Puzzle = Puzzle
module.exports.StreamedCatParsingResult = StreamedCatParsingResult
module.exports.Cat = Cat
module.exports.CatSpend = CatSpend
module.exports.ParsedCat = ParsedCat
module.exports.Nft = Nft
module.exports.NftInfo = NftInfo
module.exports.ParsedNft = ParsedNft
module.exports.NftMetadata = NftMetadata
module.exports.NftMint = NftMint
module.exports.DidOwner = DidOwner
module.exports.MintedNfts = MintedNfts
module.exports.Did = Did
module.exports.DidInfo = DidInfo
module.exports.ParsedDid = ParsedDid
module.exports.standardPuzzleHash = standardPuzzleHash
module.exports.catPuzzleHash = catPuzzleHash
module.exports.StreamingPuzzleInfo = StreamingPuzzleInfo
module.exports.StreamedCat = StreamedCat
module.exports.Output = Output
module.exports.Pair = Pair
module.exports.CurriedProgram = CurriedProgram
module.exports.LineageProof = LineageProof
module.exports.SecretKey = SecretKey
module.exports.PublicKey = PublicKey
module.exports.Signature = Signature
module.exports.Program = Program
module.exports.Constants = Constants
module.exports.fromHex = fromHex
module.exports.toHex = toHex
module.exports.bytesEqual = bytesEqual
module.exports.treeHashAtom = treeHashAtom
module.exports.treeHashPair = treeHashPair
module.exports.sha256 = sha256
module.exports.curryTreeHash = curryTreeHash
module.exports.generateBytes = generateBytes
module.exports.Simulator = Simulator
module.exports.BlsPair = BlsPair
module.exports.BlsPairWithCoin = BlsPairWithCoin
module.exports.K1Pair = K1Pair
module.exports.R1Pair = R1Pair
module.exports.Vault = Vault
module.exports.MemberConfig = MemberConfig
module.exports.mOfNHash = mOfNHash
module.exports.k1MemberHash = k1MemberHash
module.exports.r1MemberHash = r1MemberHash
module.exports.blsMemberHash = blsMemberHash
module.exports.passkeyMemberHash = passkeyMemberHash
module.exports.singletonMemberHash = singletonMemberHash
module.exports.fixedMemberHash = fixedMemberHash
module.exports.customMemberHash = customMemberHash
module.exports.Restriction = Restriction
module.exports.RestrictionKind = RestrictionKind
module.exports.timelockRestriction = timelockRestriction
module.exports.force1Of2Restriction = force1Of2Restriction
module.exports.preventConditionOpcodeRestriction = preventConditionOpcodeRestriction
module.exports.preventMultipleCreateCoinsRestriction = preventMultipleCreateCoinsRestriction
module.exports.preventSideEffectsRestriction = preventSideEffectsRestriction
module.exports.MipsSpend = MipsSpend
module.exports.VaultMint = VaultMint
module.exports.wrappedDelegatedPuzzleHash = wrappedDelegatedPuzzleHash
module.exports.Coin = Coin
module.exports.CoinSpend = CoinSpend
module.exports.SpendBundle = SpendBundle
module.exports.Spend = Spend
module.exports.CoinsetClient = CoinsetClient
module.exports.BlockchainStateResponse = BlockchainStateResponse
module.exports.BlockchainState = BlockchainState
module.exports.MempoolMinFees = MempoolMinFees
module.exports.SyncState = SyncState
module.exports.AdditionsAndRemovalsResponse = AdditionsAndRemovalsResponse
module.exports.GetBlockResponse = GetBlockResponse
module.exports.GetBlockRecordResponse = GetBlockRecordResponse
module.exports.GetBlockRecordsResponse = GetBlockRecordsResponse
module.exports.GetBlocksResponse = GetBlocksResponse
module.exports.GetBlockSpendsResponse = GetBlockSpendsResponse
module.exports.GetCoinRecordResponse = GetCoinRecordResponse
module.exports.GetCoinRecordsResponse = GetCoinRecordsResponse
module.exports.GetPuzzleAndSolutionResponse = GetPuzzleAndSolutionResponse
module.exports.PushTxResponse = PushTxResponse
module.exports.GetNetworkInfoResponse = GetNetworkInfoResponse
module.exports.GetMempoolItemResponse = GetMempoolItemResponse
module.exports.GetMempoolItemsResponse = GetMempoolItemsResponse
module.exports.CoinRecord = CoinRecord
module.exports.MempoolItem = MempoolItem
module.exports.FullBlock = FullBlock
module.exports.EndOfSubSlotBundle = EndOfSubSlotBundle
module.exports.ChallengeChainSubSlot = ChallengeChainSubSlot
module.exports.InfusedChallengeChainSubSlot = InfusedChallengeChainSubSlot
module.exports.RewardChainSubSlot = RewardChainSubSlot
module.exports.SubSlotProofs = SubSlotProofs
module.exports.VdfInfo = VdfInfo
module.exports.VdfProof = VdfProof
module.exports.TransactionsInfo = TransactionsInfo
module.exports.RewardChainBlock = RewardChainBlock
module.exports.FoliageTransactionBlock = FoliageTransactionBlock
module.exports.FoliageBlockData = FoliageBlockData
module.exports.Foliage = Foliage
module.exports.PoolTarget = PoolTarget
module.exports.BlockRecord = BlockRecord
module.exports.ProofOfSpace = ProofOfSpace
module.exports.SubEpochSummary = SubEpochSummary
module.exports.Address = Address
module.exports.K1SecretKey = K1SecretKey
module.exports.K1PublicKey = K1PublicKey
module.exports.K1Signature = K1Signature
module.exports.R1SecretKey = R1SecretKey
module.exports.R1PublicKey = R1PublicKey
module.exports.R1Signature = R1Signature
module.exports.Clvm = Clvm
module.exports.Remark = Remark
module.exports.AggSigParent = AggSigParent
module.exports.AggSigPuzzle = AggSigPuzzle
module.exports.AggSigAmount = AggSigAmount
module.exports.AggSigPuzzleAmount = AggSigPuzzleAmount
module.exports.AggSigParentAmount = AggSigParentAmount
module.exports.AggSigParentPuzzle = AggSigParentPuzzle
module.exports.AggSigUnsafe = AggSigUnsafe
module.exports.AggSigMe = AggSigMe
module.exports.CreateCoin = CreateCoin
module.exports.ReserveFee = ReserveFee
module.exports.CreateCoinAnnouncement = CreateCoinAnnouncement
module.exports.CreatePuzzleAnnouncement = CreatePuzzleAnnouncement
module.exports.AssertCoinAnnouncement = AssertCoinAnnouncement
module.exports.AssertPuzzleAnnouncement = AssertPuzzleAnnouncement
module.exports.AssertConcurrentSpend = AssertConcurrentSpend
module.exports.AssertConcurrentPuzzle = AssertConcurrentPuzzle
module.exports.AssertSecondsRelative = AssertSecondsRelative
module.exports.AssertSecondsAbsolute = AssertSecondsAbsolute
module.exports.AssertHeightRelative = AssertHeightRelative
module.exports.AssertHeightAbsolute = AssertHeightAbsolute
module.exports.AssertBeforeSecondsRelative = AssertBeforeSecondsRelative
module.exports.AssertBeforeSecondsAbsolute = AssertBeforeSecondsAbsolute
module.exports.AssertBeforeHeightRelative = AssertBeforeHeightRelative
module.exports.AssertBeforeHeightAbsolute = AssertBeforeHeightAbsolute
module.exports.AssertMyCoinId = AssertMyCoinId
module.exports.AssertMyParentId = AssertMyParentId
module.exports.AssertMyPuzzleHash = AssertMyPuzzleHash
module.exports.AssertMyAmount = AssertMyAmount
module.exports.AssertMyBirthSeconds = AssertMyBirthSeconds
module.exports.AssertMyBirthHeight = AssertMyBirthHeight
module.exports.AssertEphemeral = AssertEphemeral
module.exports.SendMessage = SendMessage
module.exports.ReceiveMessage = ReceiveMessage
module.exports.Softfork = Softfork
module.exports.Mnemonic = Mnemonic
