use chia::protocol::Bytes32;

use crate::VerifiedData;

#[derive(Debug, Clone, PartialEq, Eq)]
#[must_use]
pub struct VerificationInfo {
    pub launcher_id: Bytes32,

    pub revocation_singleton_launcher_id: Bytes32,
    pub verified_data: VerifiedData,
}

impl VerificationInfo {
    pub fn new(
        launcher_id: Bytes32,
        revocation_singleton_launcher_id: Bytes32,
        verified_data: VerifiedData,
    ) -> Self {
        Self {
            launcher_id,
            revocation_singleton_launcher_id,
            verified_data,
        }
    }
}
