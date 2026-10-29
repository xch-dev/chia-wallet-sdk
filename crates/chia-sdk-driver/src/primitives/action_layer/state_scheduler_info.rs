use chia_protocol::{Bytes, Bytes32};
use chia_puzzle_types::Memos;
use chia_puzzle_types::singleton::{LauncherSolution, SingletonArgs, SingletonStruct};
use chia_sdk_types::Condition;
use chia_sdk_types::puzzles::StateSchedulerLayerArgs;
use clvm_traits::{FromClvm, ToClvm, clvm_quote};
use clvm_utils::{ToTreeHash, TreeHash};
use clvmr::{Allocator, NodePtr};

use crate::{
    DriverError, SingletonLayer, StateSchedulerLayer, XchandlesRegistryReceivedMessagePrefix,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StateSchedulerInfo<S> {
    pub launcher_id: Bytes32,

    pub receiver_singleton_launcher_id: Bytes32,
    /// Nonempty, strictly increasing list of `(unix timestamp, state)` entries.
    pub state_schedule: Vec<(u64, S)>,
    pub generation: usize,
    pub final_puzzle_hash: Bytes32,
}

impl<S> StateSchedulerInfo<S>
where
    S: ToTreeHash + Clone,
{
    pub fn new(
        launcher_id: Bytes32,
        receiver_singleton_launcher_id: Bytes32,
        state_schedule: Vec<(u64, S)>,
        generation: usize,
        final_puzzle_hash: Bytes32,
    ) -> Result<Self, DriverError> {
        validate_state_schedule(&state_schedule)?;

        Ok(Self {
            launcher_id,
            receiver_singleton_launcher_id,
            state_schedule,
            generation,
            final_puzzle_hash,
        })
    }

    #[must_use]
    pub fn with_generation(&self, generation: usize) -> Self {
        Self {
            generation,
            ..self.clone()
        }
    }

    pub fn inner_puzzle_hash_for(
        &self,
        next_puzzle_hash: Bytes32,
        required_timestamp: u64,
        prefix_and_message_hash: TreeHash,
    ) -> TreeHash {
        StateSchedulerLayerArgs::<TreeHash, _>::curry_tree_hash(
            SingletonStruct::new(self.receiver_singleton_launcher_id)
                .tree_hash()
                .into(),
            prefix_and_message_hash,
            &clvm_quote!(vec![
                Condition::<()>::create_coin(next_puzzle_hash, 1, Memos::None),
                Condition::assert_seconds_absolute(required_timestamp),
            ]),
        )
    }

    pub fn inner_puzzle_hash_for_generation(&self, generation: usize) -> TreeHash {
        if generation >= self.state_schedule.len() {
            return self.final_puzzle_hash.into();
        }

        let mut inner_puzzle_hash: TreeHash = self.final_puzzle_hash.into();

        let mut i = self.state_schedule.len();
        while i > generation {
            let prefix_and_message_hash: Bytes =
                XchandlesRegistryReceivedMessagePrefix::update_state(
                    self.state_schedule[i - 1].1.tree_hash(),
                )
                .into();
            inner_puzzle_hash = self.inner_puzzle_hash_for(
                inner_puzzle_hash.into(),
                self.state_schedule[i - 1].0,
                prefix_and_message_hash.tree_hash(),
            );

            i -= 1;
        }

        inner_puzzle_hash
    }

    pub fn inner_puzzle_hash(&self) -> TreeHash {
        self.inner_puzzle_hash_for_generation(self.generation)
    }

    pub fn into_layers(self) -> SingletonLayer<StateSchedulerLayer> {
        let (required_timestamp, new_state) = self.state_schedule[self.generation].clone();

        SingletonLayer::new(
            self.launcher_id,
            StateSchedulerLayer::new(
                SingletonStruct::new(self.receiver_singleton_launcher_id)
                    .tree_hash()
                    .into(),
                new_state.tree_hash().into(),
                required_timestamp,
                self.inner_puzzle_hash_for_generation(self.generation + 1)
                    .into(),
            ),
        )
    }

    pub fn from_launcher_solution<H>(
        allocator: &mut Allocator,
        laucher_solution: LauncherSolution<NodePtr>,
    ) -> Result<Option<(Self, H)>, DriverError>
    where
        S: FromClvm<Allocator>,
        H: FromClvm<Allocator>,
    {
        let hints = StateSchedulerLauncherHints::<S, H>::from_clvm(
            allocator,
            laucher_solution.key_value_list,
        )?;

        let candidate = Self::new(
            hints.my_launcher_id,
            hints.receiver_singleton_launcher_id,
            hints.state_schedule,
            0,
            hints.final_puzzle_hash,
        )?;

        let predicted_inner_puzzle_hash = candidate.inner_puzzle_hash();
        let predicted_puzzle_hash =
            SingletonArgs::curry_tree_hash(hints.my_launcher_id, predicted_inner_puzzle_hash);

        if laucher_solution.amount == 1
            && laucher_solution.singleton_puzzle_hash == predicted_puzzle_hash.into()
        {
            Ok(Some((candidate, hints.final_puzzle_hash_hints)))
        } else {
            Ok(None)
        }
    }

    pub fn to_hints<H>(&self, final_puzzle_hash_hints: H) -> StateSchedulerLauncherHints<S, H> {
        StateSchedulerLauncherHints {
            my_launcher_id: self.launcher_id,
            receiver_singleton_launcher_id: self.receiver_singleton_launcher_id,
            final_puzzle_hash: self.final_puzzle_hash,
            state_schedule: self.state_schedule.clone(),
            final_puzzle_hash_hints,
        }
    }
}

/// Launcher hints retain their previous CLVM structure; schedule values are Unix timestamps.
#[derive(ToClvm, FromClvm, Debug, Clone, PartialEq, Eq)]
#[clvm(curry)]
pub struct StateSchedulerLauncherHints<S, H> {
    pub my_launcher_id: Bytes32,
    pub receiver_singleton_launcher_id: Bytes32,
    pub final_puzzle_hash: Bytes32,
    pub state_schedule: Vec<(u64, S)>,
    #[clvm(rest)]
    pub final_puzzle_hash_hints: H,
}

fn validate_state_schedule<S>(state_schedule: &[(u64, S)]) -> Result<(), DriverError> {
    if state_schedule.is_empty() {
        return Err(DriverError::InvalidStateSchedule);
    }

    for window in state_schedule.windows(2) {
        if window[1].0 <= window[0].0 {
            return Err(DriverError::InvalidStateSchedule);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use chia_protocol::Bytes32;
    use chia_puzzle_types::Memos;
    use chia_sdk_types::Condition;
    use clvm_traits::{FromClvm, ToClvm, clvm_quote};
    use clvm_utils::ToTreeHash;
    use clvmr::Allocator;

    use crate::{CatalogRegistryState, DriverError};

    use super::*;

    fn mock_state(generator: u8) -> CatalogRegistryState {
        CatalogRegistryState {
            cat_maker_puzzle_hash: Bytes32::new([generator; 32]),
            registration_price: u64::from(generator) * 1000,
        }
    }

    #[test]
    fn test_rejects_empty_schedule() {
        let err = StateSchedulerInfo::new(
            Bytes32::default(),
            Bytes32::default(),
            Vec::<(u64, CatalogRegistryState)>::new(),
            0,
            Bytes32::default(),
        )
        .unwrap_err();
        assert!(matches!(err, DriverError::InvalidStateSchedule));
    }

    #[test]
    fn test_rejects_duplicate_timestamps() {
        let err = StateSchedulerInfo::new(
            Bytes32::default(),
            Bytes32::default(),
            vec![(100, mock_state(0)), (100, mock_state(1))],
            0,
            Bytes32::default(),
        )
        .unwrap_err();
        assert!(matches!(err, DriverError::InvalidStateSchedule));
    }

    #[test]
    fn test_rejects_non_increasing_timestamps() {
        let err = StateSchedulerInfo::new(
            Bytes32::default(),
            Bytes32::default(),
            vec![(200, mock_state(0)), (150, mock_state(1))],
            0,
            Bytes32::default(),
        )
        .unwrap_err();
        assert!(matches!(err, DriverError::InvalidStateSchedule));
    }

    #[test]
    fn test_accepts_strictly_increasing_timestamps() -> anyhow::Result<()> {
        let info = StateSchedulerInfo::new(
            Bytes32::new([1; 32]),
            Bytes32::new([2; 32]),
            vec![(100, mock_state(0)), (200, mock_state(1))],
            0,
            Bytes32::new([3; 32]),
        )?;
        assert_eq!(info.state_schedule.len(), 2);
        Ok(())
    }

    #[test]
    fn test_inner_puzzle_hash_uses_assert_seconds_absolute() -> anyhow::Result<()> {
        let info = StateSchedulerInfo::new(
            Bytes32::new([1; 32]),
            Bytes32::new([2; 32]),
            vec![(1_700_000_000, mock_state(0))],
            0,
            Bytes32::new([3; 32]),
        )?;

        let seconds_hash = info.inner_puzzle_hash();

        let prefix_and_message: Bytes = XchandlesRegistryReceivedMessagePrefix::update_state(
            info.state_schedule[0].1.tree_hash(),
        )
        .into();
        let height_hash = StateSchedulerLayerArgs::<TreeHash, _>::curry_tree_hash(
            SingletonStruct::new(info.receiver_singleton_launcher_id)
                .tree_hash()
                .into(),
            prefix_and_message.tree_hash(),
            &clvm_quote!(vec![
                Condition::<()>::create_coin(info.final_puzzle_hash, 1, Memos::None),
                Condition::assert_height_absolute(1_700_000_000),
            ]),
        );

        assert_ne!(hex::encode(seconds_hash), hex::encode(height_hash));
        assert_eq!(
            hex::encode(seconds_hash),
            hex::encode(info.inner_puzzle_hash_for(
                info.final_puzzle_hash,
                1_700_000_000,
                prefix_and_message.tree_hash(),
            ))
        );

        Ok(())
    }

    #[test]
    fn test_launcher_hints_roundtrip() -> anyhow::Result<()> {
        let schedule = vec![(100, mock_state(0)), (200, mock_state(1))];
        let info = StateSchedulerInfo::new(
            Bytes32::new([9; 32]),
            Bytes32::new([8; 32]),
            schedule,
            0,
            Bytes32::new([7; 32]),
        )?;
        let hints = info.to_hints(NodePtr::NIL);

        let mut allocator = Allocator::new();
        let ptr = hints.to_clvm(&mut allocator)?;
        let roundtrip = StateSchedulerLauncherHints::<CatalogRegistryState, NodePtr>::from_clvm(
            &allocator, ptr,
        )?;

        assert_eq!(roundtrip.my_launcher_id, hints.my_launcher_id);
        assert_eq!(
            roundtrip.receiver_singleton_launcher_id,
            hints.receiver_singleton_launcher_id
        );
        assert_eq!(roundtrip.final_puzzle_hash, hints.final_puzzle_hash);
        assert_eq!(roundtrip.state_schedule, hints.state_schedule);
        assert_eq!(roundtrip.final_puzzle_hash_hints, NodePtr::NIL);

        Ok(())
    }
}
