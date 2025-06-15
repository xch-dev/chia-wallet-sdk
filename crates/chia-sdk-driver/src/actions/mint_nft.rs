use chia_protocol::Bytes32;

use crate::{
    Deltas, DriverError, FungibleAsset, HashedPtr, Id, SingletonSpends, SpendAction, SpendContext,
    SpendKind, Spends,
};

#[derive(Debug, Clone, Copy)]
pub struct MintNftAction {
    pub parent_did_id: Option<Id>,
    pub metadata: HashedPtr,
    pub metadata_updater_puzzle_hash: Bytes32,
    pub royalty_puzzle_hash: Bytes32,
    pub royalty_basis_points: u16,
    pub amount: u64,
}

impl MintNftAction {
    pub fn new(
        parent_did_id: Option<Id>,
        metadata: HashedPtr,
        metadata_updater_puzzle_hash: Bytes32,
        royalty_puzzle_hash: Bytes32,
        royalty_basis_points: u16,
        amount: u64,
    ) -> Self {
        Self {
            parent_did_id,
            metadata,
            metadata_updater_puzzle_hash,
            royalty_puzzle_hash,
            royalty_basis_points,
            amount,
        }
    }
}

impl Default for MintNftAction {
    fn default() -> Self {
        Self::new(
            None,
            HashedPtr::NIL,
            Bytes32::default(),
            Bytes32::default(),
            0,
            1,
        )
    }
}

impl SpendAction for MintNftAction {
    fn calculate_delta(&self, deltas: &mut Deltas, index: usize) {
        deltas.update(None).output += self.amount;
        deltas.update(Some(Id::New(index))).input += self.amount;

        if let Some(did_id) = self.parent_did_id {
            let did = deltas.update(Some(did_id));
            did.input += 1;
            did.output += 1;
        } else {
            deltas.set_xch_needed();
        }
    }

    fn spend(
        &self,
        ctx: &mut SpendContext,
        spends: &mut Spends,
        index: usize,
    ) -> Result<(), DriverError> {
        let (source, launcher) = spends.xch.create_launcher(self.amount)?;
        let source = &mut spends.xch.items[source];

        let (parent_conditions, eve_nft) = launcher.mint_eve_nft(
            ctx,
            source.asset.p2_puzzle_hash(),
            self.metadata,
            self.metadata_updater_puzzle_hash,
            self.royalty_puzzle_hash,
            self.royalty_basis_points,
        )?;

        match &mut source.kind {
            SpendKind::Conditions(spend) => {
                spend.add_conditions(parent_conditions)?;
            }
        }

        let kind = source.kind.child();

        spends
            .nfts
            .insert(Id::New(index), SingletonSpends::new(eve_nft, kind, true));

        Ok(())
    }
}
