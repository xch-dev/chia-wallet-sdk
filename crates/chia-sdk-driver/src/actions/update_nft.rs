use chia_sdk_types::conditions::TransferNft;

use crate::{Deltas, DriverError, Id, Spend, SpendAction, SpendContext, Spends};

#[derive(Debug, Clone)]
pub struct UpdateNftAction {
    pub id: Id,
    pub metadata_update_spends: Vec<Spend>,
    pub transfer_condition: Option<TransferNft>,
}

impl UpdateNftAction {
    pub fn new(
        id: Id,
        metadata_update_spends: Vec<Spend>,
        transfer_condition: Option<TransferNft>,
    ) -> Self {
        Self {
            id,
            metadata_update_spends,
            transfer_condition,
        }
    }
}

impl SpendAction for UpdateNftAction {
    fn calculate_delta(&self, deltas: &mut Deltas, _index: usize) {
        let nft = deltas.update(Some(self.id));
        nft.input += 1;
        nft.output += 1;
    }

    fn spend(
        &self,
        _ctx: &mut SpendContext,
        spends: &mut Spends,
        _index: usize,
    ) -> Result<(), DriverError> {
        let nft = spends
            .nfts
            .get_mut(&self.id)
            .ok_or(DriverError::InvalidAssetId)?
            .last_mut()?;

        nft.child_info
            .metadata_update_spends
            .extend_from_slice(&self.metadata_update_spends);

        if let Some(transfer_condition) = self.transfer_condition.clone() {
            nft.child_info.transfer_condition = Some(transfer_condition);
        }

        Ok(())
    }
}
