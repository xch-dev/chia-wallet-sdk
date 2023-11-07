use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait DerivationWallet {
    fn derivation_index(&self, puzzle_hash: [u8; 32]) -> Option<u32>;
    fn unused_derivation_index(&self) -> Option<u32>;
    fn next_derivation_index(&self) -> u32;

    async fn generate_puzzle_hashes(&self, puzzle_hashes: u32) -> Result<Vec<[u8; 32]>>;

    async fn sync(&self, gap: u32) -> Result<u32> {
        // If there aren't any derivations, generate the first batch.
        if self.next_derivation_index() == 0 {
            self.generate_puzzle_hashes(gap).await?;
        }

        loop {
            match self.unused_derivation_index() {
                // Check if an unused derivation index was found.
                Some(unused_index) => {
                    // If so, calculate the extra unused derivations after that index.
                    let last_index = self.next_derivation_index() - 1;
                    let extra_indices = last_index - unused_index;

                    // Make sure at least `gap` indices are available if needed.
                    if extra_indices < gap {
                        self.generate_puzzle_hashes(gap).await?;
                    }

                    // Return the unused derivation index.
                    return Ok(unused_index);
                }
                // Otherwise, generate more puzzle hashes and check again.
                None => {
                    self.generate_puzzle_hashes(gap).await?;
                }
            }
        }
    }
}
