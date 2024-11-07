use std::collections::HashMap;

use chia_protocol::ProtocolMessageTypes;
use once_cell::sync::Lazy;

#[derive(Debug, Clone)]
pub struct RateLimits {
    pub default_settings: RateLimit,
    pub non_tx_frequency: f64,
    pub non_tx_max_total_size: f64,
    pub tx: HashMap<ProtocolMessageTypes, RateLimit>,
    pub other: HashMap<ProtocolMessageTypes, RateLimit>,
}

impl RateLimits {
    pub fn extend(&mut self, other: &Self) {
        self.default_settings = other.default_settings;
        self.non_tx_frequency = other.non_tx_frequency;
        self.non_tx_max_total_size = other.non_tx_max_total_size;
        self.tx.extend(other.tx.clone());
        self.other.extend(other.other.clone());
    }
}

#[derive(Debug, Clone, Copy)]
pub struct RateLimit {
    pub frequency: f64,
    pub max_size: f64,
    pub max_total_size: Option<f64>,
}

impl RateLimit {
    pub fn new(frequency: f64, max_size: f64, max_total_size: Option<f64>) -> Self {
        Self {
            frequency,
            max_size,
            max_total_size,
        }
    }
}

macro_rules! settings {
    ($($message:ident => $frequency:expr, $max_size:expr $(, $max_total_size:expr)? ;)*) => {
        {
            let mut settings = HashMap::new();
            $(
                #[allow(unused_mut, unused_assignments)]
                let mut max_total_size = None;
                $( max_total_size = Some($max_total_size); )?
                settings.insert(
                    ProtocolMessageTypes::$message,
                    RateLimit::new(
                        $frequency.into(),
                        $max_size.into(),
                        max_total_size.map(|num: u32| num.into()),
                    )
                );
            )*
            settings
        }
    };
}

// TODO: Fix commented out rate limits.
pub static V1_RATE_LIMITS: Lazy<RateLimits> = Lazy::new(|| RateLimits {
    default_settings: RateLimit::new(100.0, 1024.0 * 1024.0, Some(100.0 * 1024.0 * 1024.0)),
    non_tx_frequency: 1000.0,
    non_tx_max_total_size: 100.0 * 1024.0 * 1024.0,
    tx: settings! {
        NewTransaction => 5000, 100, 5000 * 100;
        RequestTransaction => 5000, 100, 5000 * 100;
        RespondTransaction => 5000, 1024 * 1024, 20 * 1024 * 1024;
        SendTransaction => 5000, 1024 * 1024;
        TransactionAck => 5000, 2048;
    },
    other: settings! {
        Handshake => 5, 10 * 1024, 5 * 10 * 1024;
        HarvesterHandshake => 5, 1024 * 1024;
        NewSignagePointHarvester => 100, 4886;
        NewProofOfSpace => 100, 2048;
        RequestSignatures => 100, 2048;
        RespondSignatures => 100, 2048;
        NewSignagePoint => 200, 2048;
        DeclareProofOfSpace => 100, 10 * 1024;
        RequestSignedValues => 100, 10 * 1024;
        FarmingInfo => 100, 1024;
        SignedValues => 100, 1024;
        NewPeakTimelord => 100, 20 * 1024;
        NewUnfinishedBlockTimelord => 100, 10 * 1024;
        NewSignagePointVdf => 100, 100 * 1024;
        NewInfusionPointVdf => 100, 100 * 1024;
        NewEndOfSubSlotVdf => 100, 100 * 1024;
        RequestCompactProofOfTime => 100, 10 * 1024;
        RespondCompactProofOfTime => 100, 100 * 1024;
        NewPeak => 200, 512;
        RequestProofOfWeight => 5, 100;
        RespondProofOfWeight => 5, 50 * 1024 * 1024, 100 * 1024 * 1024;
        RequestBlock => 200, 100;
        RejectBlock => 200, 100;
        RequestBlocks => 500, 100;
        RespondBlocks => 100, 50 * 1024 * 1024, 5 * 50 * 1024 * 1024;
        RejectBlocks => 100, 100;
        RespondBlock => 200, 2 * 1024 * 1024, 10 * 2 * 1024 * 1024;
        NewUnfinishedBlock => 200, 100;
        RequestUnfinishedBlock => 200, 100;
        NewUnfinishedBlock2 => 200, 100;
        RequestUnfinishedBlock2 => 200, 100;
        RespondUnfinishedBlock => 200, 2 * 1024 * 1024, 10 * 2 * 1024 * 1024;
        NewSignagePointOrEndOfSubSlot => 200, 200;
        RequestSignagePointOrEndOfSubSlot => 200, 200;
        RespondSignagePoint => 200, 50 * 1024;
        RespondEndOfSubSlot => 100, 50 * 1024;
        RequestMempoolTransactions => 5, 1024 * 1024;
        RequestCompactVDF => 200, 1024;
        RespondCompactVDF => 200, 100 * 1024;
        NewCompactVDF => 100, 1024;
        RequestPeers => 10, 100;
        RespondPeers => 10, 1024 * 1024;
        RequestPuzzleSolution => 1000, 100;
        RespondPuzzleSolution => 1000, 1024 * 1024;
        RejectPuzzleSolution => 1000, 100;
        NewPeakWallet => 200, 300;
        RequestBlockHeader => 500, 100;
        RespondBlockHeader => 500, 500 * 1024;
        RejectHeaderRequest => 500, 100;
        RequestRemovals => 500, 50 * 1024, 10 * 1024 * 1024;
        RespondRemovals => 500, 1024 * 1024, 10 * 1024 * 1024;
        RejectRemovalsRequest => 500, 100;
        RequestAdditions => 500, 1024 * 1024, 10 * 1024 * 1024;
        RespondAdditions => 500, 1024 * 1024, 10 * 1024 * 1024;
        RejectAdditionsRequest => 500, 100;
        RequestHeaderBlocks => 500, 100;
        RejectHeaderBlocks => 100, 100;
        RespondHeaderBlocks => 500, 2 * 1024 * 1024, 100 * 1024 * 1024;
        RequestPeersIntroducer => 100, 100;
        RespondPeersIntroducer => 100, 1024 * 1024;
        FarmNewBlock => 200, 200;
        RequestPlots => 10, 10 * 1024 * 1024;
        RespondPlots => 10, 100 * 1024 * 1024;
        PlotSyncStart => 1000, 100 * 1024 * 1024;
        PlotSyncLoaded => 1000, 100 * 1024 * 1024;
        PlotSyncRemoved => 1000, 100 * 1024 * 1024;
        PlotSyncInvalid => 1000, 100 * 1024 * 1024;
        PlotSyncKeysMissing => 1000, 100 * 1024 * 1024;
        PlotSyncDuplicates => 1000, 100 * 1024 * 1024;
        PlotSyncDone => 1000, 100 * 1024 * 1024;
        PlotSyncResponse => 3000, 100 * 1024 * 1024;
        CoinStateUpdate => 1000, 100 * 1024 * 1024;
        RegisterForPhUpdates => 1000, 100 * 1024 * 1024;
        RespondToPhUpdates => 1000, 100 * 1024 * 1024;
        RegisterForCoinUpdates => 1000, 100 * 1024 * 1024;
        RespondToCoinUpdates => 1000, 100 * 1024 * 1024;
        RequestRemovePuzzleSubscriptions => 1000, 100 * 1024 * 1024;
        RespondRemovePuzzleSubscriptions => 1000, 100 * 1024 * 1024;
        RequestRemoveCoinSubscriptions => 1000, 100 * 1024 * 1024;
        RespondRemoveCoinSubscriptions => 1000, 100 * 1024 * 1024;
        RequestPuzzleState => 1000, 100 * 1024 * 1024;
        RespondPuzzleState => 1000, 100 * 1024 * 1024;
        RejectPuzzleState => 200, 100;
        RequestCoinState => 1000, 100 * 1024 * 1024;
        RespondCoinState => 1000, 100 * 1024 * 1024;
        RejectCoinState => 200, 100;
        // MempoolItemsAdded => 1000, 100 * 1024 * 1024;
        // MempoolItemsRemoved => 1000, 100 * 1024 * 1024;
        // RequestCostInfo => 1000, 100;
        // RespondCostInfo => 1000, 1024;
        // RequestSesHashes => 2000, 1 * 1024 * 1024;
        // RespondSesHashes => 2000, 1 * 1024 * 1024;
        RequestChildren => 2000, 1024 * 1024;
        RespondChildren => 2000, 1024 * 1024;
    },
});

// TODO: Fix commented out rate limits.
// Also, why are these in tx?
static V2_RATE_LIMIT_CHANGES: Lazy<RateLimits> = Lazy::new(|| RateLimits {
    default_settings: RateLimit::new(100.0, 1024.0 * 1024.0, Some(100.0 * 1024.0 * 1024.0)),
    non_tx_frequency: 1000.0,
    non_tx_max_total_size: 100.0 * 1024.0 * 1024.0,
    tx: settings! {
        RequestBlockHeader => 500, 100;
        RespondBlockHeader => 500, 500 * 1024;
        RejectHeaderRequest => 500, 100;
        RequestRemovals => 5000, 50 * 1024, 10 * 1024 * 1024;
        RespondRemovals => 5000, 1024 * 1024, 10 * 1024 * 1024;
        RejectRemovalsRequest => 500, 100;
        RequestAdditions => 50000, 100 * 1024 * 1024;
        RespondAdditions => 50000, 100 * 1024 * 1024;
        RejectAdditionsRequest => 500, 100;
        RejectHeaderBlocks => 1000, 100;
        RespondHeaderBlocks => 5000, 2 * 1024 * 1024;
        RequestBlockHeaders => 5000, 100;
        RejectBlockHeaders => 1000, 100;
        RespondBlockHeaders => 5000, 2 * 1024 * 1024;
        // RequestSesHashes => 2000, 1 * 1024 * 1024;
        // RespondSesHashes => 2000, 1 * 1024 * 1024;
        RequestChildren => 2000, 1024 * 1024;
        RespondChildren => 2000, 1024 * 1024;
        RequestPuzzleSolution => 5000, 100;
        RespondPuzzleSolution => 5000, 1024 * 1024;
        RejectPuzzleSolution => 5000, 100;
        NoneResponse => 500, 100;
        // Error => 50000, 100;
    },
    other: settings! {
        RequestHeaderBlocks => 5000, 100;
    },
});

pub static V2_RATE_LIMITS: Lazy<RateLimits> = Lazy::new(|| {
    let mut rate_limits = V1_RATE_LIMITS.clone();
    rate_limits.extend(&V2_RATE_LIMIT_CHANGES);
    rate_limits
});
