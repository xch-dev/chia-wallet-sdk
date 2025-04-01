use chia_protocol::{Bytes32, SpendBundle};
use serde::{de::DeserializeOwned, Serialize};
use std::future::Future;

use super::{
    AdditionsAndRemovalsResponse, BlockchainStateResponse, GetBlockRecordByHeightResponse,
    GetBlockRecordResponse, GetBlockRecordsResponse, GetBlockResponse, GetBlockSpendsResponse,
    GetBlocksResponse, GetCoinRecordResponse, GetCoinRecordsResponse, GetMempoolItemResponse,
    GetMempoolItemsResponse, GetNetworkInfoResponse, GetPuzzleAndSolutionResponse, PushTxResponse,
};

pub trait ChiaRpcClient {
    type Error;

    fn base_url(&self) -> &str;

    fn make_post_request<R, B>(
        &self,
        endpoint: &str,
        body: B,
    ) -> impl Future<Output = Result<R, Self::Error>>
    where
        B: Serialize + Send,
        R: DeserializeOwned + Send;

    fn get_blockchain_state(
        &self,
    ) -> impl Future<Output = Result<BlockchainStateResponse, Self::Error>> {
        self.make_post_request("get_blockchain_state", serde_json::json!({}))
    }

    fn get_additions_and_removals(
        &self,
        header_hash: Bytes32,
    ) -> impl Future<Output = Result<AdditionsAndRemovalsResponse, Self::Error>> {
        self.make_post_request(
            "get_additions_and_removals",
            serde_json::json!({
                "header_hash": format!("0x{}", hex::encode(header_hash.to_bytes())),
            }),
        )
    }

    fn get_block(
        &self,
        header_hash: Bytes32,
    ) -> impl Future<Output = Result<GetBlockResponse, Self::Error>> {
        self.make_post_request(
            "get_block",
            serde_json::json!({
                "header_hash": format!("0x{}", hex::encode(header_hash.to_bytes())),
            }),
        )
    }

    fn get_block_record(
        &self,
        header_hash: Bytes32,
    ) -> impl Future<Output = Result<GetBlockRecordResponse, Self::Error>> {
        self.make_post_request(
            "get_block_record",
            serde_json::json!({
                "header_hash": format!("0x{}", hex::encode(header_hash.to_bytes())),
            }),
        )
    }

    fn get_block_record_by_height(
        &self,
        height: u32,
    ) -> impl Future<Output = Result<GetBlockRecordByHeightResponse, Self::Error>> {
        self.make_post_request(
            "get_block_record_by_height",
            serde_json::json!({
                "height": height,
            }),
        )
    }

    fn get_block_records(
        &self,
        start_height: u32,
        end_height: u32,
    ) -> impl Future<Output = Result<GetBlockRecordsResponse, Self::Error>> {
        self.make_post_request(
            "get_block_records",
            serde_json::json!({
                "start_height": start_height,
                "end_height": end_height,
            }),
        )
    }

    fn get_blocks(
        &self,
        start: u32,
        end: u32,
        exclude_header_hash: bool,
        exclude_reorged: bool,
    ) -> impl Future<Output = Result<GetBlocksResponse, Self::Error>> {
        self.make_post_request(
            "get_blocks",
            serde_json::json!({
                "start": start,
                "end": end,
                "exclude_header_hash": exclude_header_hash,
                "exclude_reorged": exclude_reorged,
            }),
        )
    }

    fn get_block_spends(
        &self,
        header_hash: Bytes32,
    ) -> impl Future<Output = Result<GetBlockSpendsResponse, Self::Error>> {
        self.make_post_request(
            "get_block_spends",
            serde_json::json!({
                "header_hash": format!("0x{}", hex::encode(header_hash.to_bytes())),
            }),
        )
    }

    fn get_coin_record_by_name(
        &self,
        name: Bytes32,
    ) -> impl Future<Output = Result<GetCoinRecordResponse, Self::Error>> {
        self.make_post_request(
            "get_coin_record_by_name",
            serde_json::json!({
                "name": format!("0x{}", hex::encode(name.to_bytes())),
            }),
        )
    }

    fn get_coin_records_by_hint(
        &self,
        hint: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> impl Future<Output = Result<GetCoinRecordsResponse, Self::Error>> {
        self.make_post_request(
            "get_coin_records_by_hint",
            serde_json::json!({
                "hint": format!("0x{}", hex::encode(hint.to_bytes())),
                "start_height": start_height,
                "end_height": end_height,
                "include_spent_coins": include_spent_coins,
            }),
        )
    }

    fn get_coin_records_by_names(
        &self,
        names: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> impl Future<Output = Result<GetCoinRecordsResponse, Self::Error>> {
        self.make_post_request(
            "get_coin_records_by_names",
            serde_json::json!({
                "names": names.iter().map(|name| format!("0x{}", hex::encode(name.to_bytes()))).collect::<Vec<String>>(),
                "start_height": start_height,
                "end_height": end_height,
                "include_spent_coins": include_spent_coins,
            }),
        )
    }

    fn get_coin_records_by_parent_ids(
        &self,
        parent_ids: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> impl Future<Output = Result<GetCoinRecordsResponse, Self::Error>> {
        self.make_post_request(
            "get_coin_records_by_parent_ids",
            serde_json::json!({
                "parent_ids": parent_ids.iter().map(|parent_id| format!("0x{}", hex::encode(parent_id.to_bytes()))).collect::<Vec<String>>(),
                "start_height": start_height,
                "end_height": end_height,
                "include_spent_coins": include_spent_coins,
            }),
        )
    }

    fn get_coin_records_by_puzzle_hash(
        &self,
        puzzle_hash: Bytes32,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> impl Future<Output = Result<GetCoinRecordsResponse, Self::Error>> {
        self.make_post_request(
            "get_coin_records_by_puzzle_hash",
            serde_json::json!({
                "puzzle_hash": format!("0x{}", hex::encode(puzzle_hash.to_bytes())),
                "start_height": start_height,
                "end_height": end_height,
                "include_spent_coins": include_spent_coins,
            }),
        )
    }

    fn get_coin_records_by_puzzle_hashes(
        &self,
        puzzle_hashes: Vec<Bytes32>,
        start_height: Option<u32>,
        end_height: Option<u32>,
        include_spent_coins: Option<bool>,
    ) -> impl Future<Output = Result<GetCoinRecordsResponse, Self::Error>> {
        self.make_post_request(
            "get_coin_records_by_puzzle_hashes",
            serde_json::json!({
                "puzzle_hashes": puzzle_hashes.iter().map(|puzzle_hash| format!("0x{}", hex::encode(puzzle_hash.to_bytes()))).collect::<Vec<String>>(),
                "start_height": start_height,
                "end_height": end_height,
                "include_spent_coins": include_spent_coins,
            }),
        )
    }

    fn get_puzzle_and_solution(
        &self,
        coin_id: Bytes32,
        height: Option<u32>,
    ) -> impl Future<Output = Result<GetPuzzleAndSolutionResponse, Self::Error>> {
        self.make_post_request(
            "get_puzzle_and_solution",
            serde_json::json!({
                "coin_id": format!("0x{}", hex::encode(coin_id.to_bytes())),
                "height": height,
            }),
        )
    }

    fn push_tx(
        &self,
        spend_bundle: SpendBundle,
    ) -> impl Future<Output = Result<PushTxResponse, Self::Error>> {
        self.make_post_request(
            "push_tx",
            serde_json::json!({
                "spend_bundle": {
                    "coin_spends": spend_bundle.coin_spends.iter().map(|coin_spend| {
                        serde_json::json!({
                            "coin": {
                                "amount": coin_spend.coin.amount,
                                "parent_coin_info": format!("0x{}", hex::encode(coin_spend.coin.parent_coin_info.to_bytes())),
                                "puzzle_hash": format!("0x{}", hex::encode(coin_spend.coin.puzzle_hash.to_bytes())),
                            },
                            "puzzle_reveal": format!("0x{}", hex::encode(coin_spend.puzzle_reveal.to_vec())),
                            "solution": format!("0x{}", hex::encode(coin_spend.solution.to_vec())),
                        })
                    }).collect::<Vec<serde_json::Value>>(),
                    "aggregated_signature": format!("0x{}", hex::encode(spend_bundle.aggregated_signature.to_bytes())),
                }
            }),
        )
    }

    fn get_network_info(
        &self,
    ) -> impl Future<Output = Result<GetNetworkInfoResponse, Self::Error>> {
        self.make_post_request("get_network_info", serde_json::json!({}))
    }

    fn get_mempool_item_by_tx_id(
        &self,
        tx_id: Bytes32,
    ) -> impl Future<Output = Result<GetMempoolItemResponse, Self::Error>> {
        self.make_post_request(
            "get_mempool_item_by_tx_id",
            serde_json::json!({
                "tx_id": format!("0x{}", hex::encode(tx_id.to_bytes())),
            }),
        )
    }

    fn get_mempool_items_by_coin_name(
        &self,
        coin_name: Bytes32,
    ) -> impl Future<Output = Result<GetMempoolItemsResponse, Self::Error>> {
        self.make_post_request(
            "get_mempool_items_by_coin_name",
            serde_json::json!({
                "coin_name": format!("0x{}", hex::encode(coin_name.to_bytes())),
            }),
        )
    }
}

#[cfg(test)]
mod tests {
    use chia_protocol::Coin;
    use chia_traits::Streamable;
    use hex_literal::hex;

    use crate::MockRpcClient;

    use super::*;

    #[tokio::test]
    async fn test_get_blockchain_state_success() {
        let mut client = MockRpcClient::new();

        client.mock_response(
            "http://api.example.com/get_blockchain_state",
            r#"{"blockchain_state": {"average_block_time": 18, "block_max_cost": 11000000000, "difficulty": 13504, "genesis_challenge_initialized": true, "mempool_cost": 88022711, "mempool_fees": 10, "mempool_max_total_cost": 110000000000, "mempool_min_fees": {"cost_5000000": 0}, "mempool_size": 2, "node_id": "5c8c1640aae6b0ab0f16d5ec01be46aa10ad68f8aa85446fa65f1aee9d6b0b2d", "peak": {"challenge_block_info_hash": "0x3b6cb1a7e32c8c1760ea90a11a369d04755b9d31123aa0890050869bde775150", "challenge_vdf_output": {"data": "0x03009a4b4ab74d6b1d71c1ae4a62252acceb02a517b6fc8acfd7f05632d40da1e4ee2ba51ed5383411ad59749d1f642f41b30224dcb92b8863f8b1ec89eb388dbc346d7a4e9dfdabe42f833e04bc00a4ac123c87261f6ad7477660d579b58364a1160100"}, "deficit": 15, "farmer_puzzle_hash": "0x9fbde16e03f55c85ecf94cb226083fcfe2737d4e629a981e5db3ea0eb9907af4", "fees": 300395698, "finished_challenge_slot_hashes": ["0xa321d872abefa6f935c7ec3a3b72da8f9631edd29d78cd01bc57a3fc9f0a9e46"], "finished_infused_challenge_slot_hashes": ["0xbdca81c482cebcabb532253ef990a618a2e8e8e72dee4038b54c723177d3976f"], "finished_reward_slot_hashes": ["0x77e354091551f4bbf2191513679fc50f72076dff3f2634bdb78882cc9cf43a74"], "header_hash": "0x2b525481f9330f7ca1be1ca6acdd5043362379245b16e18e5897e6203a4add3f", "height": 6515821, "infused_challenge_vdf_output": null, "overflow": false, "pool_puzzle_hash": "0x9fbde16e03f55c85ecf94cb226083fcfe2737d4e629a981e5db3ea0eb9907af4", "prev_hash": "0x5211ea6cbff7175c75355cfa3e10e447a9ee1fbcb14b0e04a6ac8462263590fd", "prev_transaction_block_hash": "0x85bc16981fae8ab0956d065092c537d2d568e9ce9c07011bbfb81bbe19ae66f7", "prev_transaction_block_height": 6515819, "required_iters": 4292961, "reward_claims_incorporated": [{"amount": 875000000000, "parent_coin_info": "0xccd5bb71183532bff220ba46c268991a00000000000000000000000000636c6b", "puzzle_hash": "0xe23046c4362b99b24e027207438717b6d5bd4d440e5a62367e053fa8b409339a"}], "reward_infusion_new_challenge": "0x5defce0af9f85a0fcf144a4f3b0364e81f576bb9c855e0efa58c2b8ba630672a", "signage_point_index": 4, "sub_epoch_summary_included": null, "sub_slot_iters": 578813952, "timestamp": 1737325862, "total_iters": 56509384327521, "weight": 35992319760}, "space": 21810833559006162944, "sub_slot_iters": 578813952, "sync": {"sync_mode": false, "sync_progress_height": 0, "sync_tip_height": 0, "synced": true}}, "success": true}"#,
        );

        let response = client.get_blockchain_state().await.unwrap();
        assert!(response.success);
        assert!(response.error.is_none());

        let state = response.blockchain_state.unwrap();
        assert_eq!(state.average_block_time, 18);
        assert_eq!(state.difficulty, 13504);
        assert_eq!(state.mempool_size, 2);

        let peak = state.peak;
        assert_eq!(peak.height, 6_515_821);
        assert_eq!(peak.deficit, 15);
        assert!(!peak.overflow);
    }

    #[tokio::test]
    async fn test_get_blockchain_state_error() {
        let mut client = MockRpcClient::new();

        client.mock_response(
            "http://api.example.com/get_blockchain_state",
            r#"{
                    "success": false,
                    "error": "Failed to connect to full node"
                }"#,
        );

        let response = client.get_blockchain_state().await.unwrap();
        assert!(!response.success);
        assert_eq!(
            response.error,
            Some("Failed to connect to full node".to_string())
        );
        assert!(response.blockchain_state.is_none());
    }

    #[tokio::test]
    async fn test_get_additions_and_removals_success() {
        let mut client = MockRpcClient::new();

        client.mock_response(
            "http://api.example.com/get_additions_and_removals",
            r#"{
                "additions": [{
                    "coin": {
                        "amount": 10019626640,
                        "parent_coin_info": "c325057d788bee13367cb8e2d71ff3e209b5e94b31b296322ba1a143053fef5b",
                        "puzzle_hash": "11cd056d9ec93f4612919b445e1ad9afeb7ef7739708c2d16cec4fd2d3cd5e63"
                    },
                    "coinbase": false,
                    "confirmed_block_index": 5910291,
                    "spent": false,
                    "spent_block_index": 0,
                    "timestamp": 1725991066
                }],
                "removals": [{
                    "coin": {
                        "amount": 1,
                        "parent_coin_info": "4dda4b8b6017c633794c2b719c3591870b4bc7682930094c11a311112c772ce6",
                        "puzzle_hash": "18cfd81a9a58d598197730b2f2a21ff3b72951577be1dcc6004080ad17069e84"
                    },
                    "coinbase": false,
                    "confirmed_block_index": 5612341,
                    "spent": true,
                    "spent_block_index": 5910291,
                    "timestamp": 1720407964
                }],
                "success": true
            }"#,
        );

        let header_hash = Bytes32::from([0x88; 32]);
        let response = client
            .get_additions_and_removals(header_hash)
            .await
            .unwrap();

        assert!(response.success);
        assert!(response.error.is_none());

        // Check additions
        let additions = response.additions.unwrap();
        assert_eq!(additions.len(), 1);
        let addition = &additions[0];
        assert_eq!(
            addition.coin,
            Coin::new(
                Bytes32::new(hex_literal::hex!(
                    "c325057d788bee13367cb8e2d71ff3e209b5e94b31b296322ba1a143053fef5b"
                )),
                Bytes32::new(hex_literal::hex!(
                    "11cd056d9ec93f4612919b445e1ad9afeb7ef7739708c2d16cec4fd2d3cd5e63"
                )),
                10_019_626_640
            )
        );
        assert!(!addition.coinbase);
        assert_eq!(addition.confirmed_block_index, 5_910_291);
        assert!(!addition.spent);
        assert_eq!(addition.spent_block_index, 0);
        assert_eq!(addition.timestamp, 1_725_991_066);

        // Check removals
        let removals = response.removals.unwrap();
        assert_eq!(removals.len(), 1);
        let removal = &removals[0];
        assert_eq!(
            removal.coin,
            Coin::new(
                Bytes32::new(hex_literal::hex!(
                    "4dda4b8b6017c633794c2b719c3591870b4bc7682930094c11a311112c772ce6"
                )),
                Bytes32::new(hex_literal::hex!(
                    "18cfd81a9a58d598197730b2f2a21ff3b72951577be1dcc6004080ad17069e84"
                )),
                1
            )
        );
        assert!(!removal.coinbase);
        assert_eq!(removal.confirmed_block_index, 5_612_341);
        assert!(removal.spent);
        assert_eq!(removal.spent_block_index, 5_910_291);
        assert_eq!(removal.timestamp, 1_720_407_964);
    }

    #[tokio::test]
    async fn test_get_additions_and_removals_error() {
        let mut client = MockRpcClient::new();

        client.mock_response(
            "http://api.example.com/get_additions_and_removals",
            r#"{
                "success": false,
                "error": "Record not found: [blah blah]"
            }"#,
        );

        let header_hash = Bytes32::from([0x88; 32]);
        let response = client
            .get_additions_and_removals(header_hash)
            .await
            .unwrap();

        assert!(!response.success);
        assert_eq!(
            response.error,
            Some("Record not found: [blah blah]".to_string())
        );
        assert!(response.additions.is_none());
        assert!(response.removals.is_none());
    }

    #[tokio::test]
    async fn test_get_block_success() {
        let mut client = MockRpcClient::new();

        client.mock_response(
            "http://api.example.com/get_block",
            r#"{
            "block": {
                "challenge_chain_ip_proof": {
                "normalized_to_identity": false,
                "witness": "0x02002e9c2d945884802a65e57541f03838956b62b74180777e30961734c7e0272794e869d2280e919e3c02b45370b68da782b0165e0b00dc55f007896e8ec15cc5142d006410b6c75bf8c9eb84a45f0bbd13f3d6fa03cbe91529d1ac710efe7c5d3a0100000000000016f300d4b9b898a5511973a0d069883c34448efc4bda3516e4cede3289b0a2851c9ad2d70100116da23efdfcb4d105de27d1a42b5bb5d0f0c52c96e01043223d4a468acd168f29791287e27e098636447ad2ff57ff5a6b9c6822ae850ff0ac62e791c2f77b2248712abe1a02bbd824949aa2c5af168dec8ec567ed39e8d5731289a2d1ca6c2b0200000000000044e8a09a1b0dcba1f40fe044b05be33c492a9621491efdd01c37240d9e03fc8c895513f10000dff075b7d5bbd49d10da4a539ff4e619664ce5b841f21a39288d477744359bef16dd979891092e3ccc04ff47e119794450cb7ec2ba3f35255d26f8717f64ba25f9d43812c5825ecbe8172b344982ce87e43595a695f59e1e069f747b973cd41c0100000000000022ca40f380ff41cd3e96d5c3259f5f6ae621843fcaa599fe84ae39d8f9cba9909466c31b0000e044c3b598f25dd1b6712ed16413d16c99713b3676d8fba95d22feedbeb180d5eea5e649246475e734adc7e9eaa5395b77754dae938b7dca46e45c802965f92d63d3caa0b4bb7962738c8c5bbf881af7a0a7c49936d67f8398b458ceb6d9c32c01000000000000682fe0a466e45feec21633653f63788ee12441f5154b000b1467896aad5d1e8a2cb87af50200465ad174b651ce5da89280823ffa965620d95ab5bc7a5992752946917414e8b07188342c47908f7a7db96124bf88478a6d2c753140131ff6df10b371ee8096293ded23eb599d8e6a3f24ada092ce16e3143e9d4a8a462e2ac9c218131bf5ed2e02000000000000229b60afba54a3f8a3cc91b454caf0d88acd93c44a39d904842857eb7ab5881a482b23290100b93bcb061cf2a36b5e69270fc435a15814f6279151ed8ebe174761f4f85c2c8fe6c293c6556c43be2efd2ba25feee728cfcd6d3d639c1782ca56272054803345452ae349fd1902a11f4463316fc5ed82b49cc4056fc8a60b2e0378eb79ac88280100000000000067d22090fdfea64f790b44bf9508f10ef8b863bd7911e98b6a86eb5457a4721f6504ece70200b49efd525fe32325ddcd8ba436cee8640864e75c5954f8edb54d99672a8f3e22f8fbe23bd34fb6e481843edebf44cce9ddbc021314f4cd7d7fb311ca2b244b47fdf966dcb3251f2b90c452391096304a429042b1a8c8e9d1e0fd55baa26a53300100000000000020f58083438a0fe47174e2dd54ed55609ebcba9526af1db814e2418155c51aaeef438d490300a1b6746a8add8cac926ebe1d21fc7da9daa798482acfa9502446cc545e2173d74b034548267df9c0e41c65bf689ac4e8bf1abe07e69be42b2eb06eb8391cd00e5f650160f3e9363fb86821ddf04cf2517b09542c327fe4c92272f86b171936050200000000000062f020e5b72f1190221c2fac3a1e09ecac22c72eb21636fec97aa40e16b80fc4673450ed0000e2164613fa906b0d193b5a7af532d811ff0927cb6707dd9d9fb36afed9c3211f645a23593b8b9fe4134e60c4435a57e827c7b6db2118e42ba143af37f3d7ad46b5b289acb00ac86bc1e7812c716f3a7142d6c38a998daa37e217e32cbce29c16010000000000000d0fc0c4efc161c7f4b1dcd004ca6b92871cba1a79e74c4215d09981681b9968a5a408fb02006d800d156b3a4520ecbe5f59def1bfbbfd0852d97ad8f57e8c0ecdae4758825de74caf7ed5532f615c6401e797e60df00bf1f970c16b8034e23c156e02982e325f3434023ec917fe0f4ac2a623868bf7162ad6769399a5df845b3e7d3b8a34420100",
                "witness_type": 9
                },
                "challenge_chain_sp_proof": {
                "normalized_to_identity": false,
                "witness": "0x0200e62e311b8380b817c4103f410adbddba77c011639a2fa8c8a157591e47c9b6a9147d5eff546f5a817ec82591e7a30038036c7ba399aafd225caa4557a9b1cd23733fb10305ac767d0188dd470379aeabece89034df58bcabc3dbd592e215dc0a010000000000000d0fc0c4efc161c7f4b1dcd004ca6b92871cba1a79e74c4215d09981681b9968a5a408fb02006d800d156b3a4520ecbe5f59def1bfbbfd0852d97ad8f57e8c0ecdae4758825de74caf7ed5532f615c6401e797e60df00bf1f970c16b8034e23c156e02982e325f3434023ec917fe0f4ac2a623868bf7162ad6769399a5df845b3e7d3b8a34420100",
                "witness_type": 1
                },
                "finished_sub_slots": [],
                "foliage": {
                "foliage_block_data": {
                    "extension_data": "0x0000000000000000000000000000000000000000000000000000000003a2c7c9",
                    "farmer_reward_puzzle_hash": "0xd86028d22a28f4d0e4ee63808492630e7829af653fd710c683980959bb7bba1d",
                    "pool_signature": null,
                    "pool_target": {
                    "max_height": 0,
                    "puzzle_hash": "0x3ac292ed271257be352c526f975a1b376752c4ed6e453ea39ed449bd7b6e3c24"
                    },
                    "unfinished_reward_block_hash": "0x8ed67dc350dff7f63b0a5900ca6baacafa4ae33082d253c7182960d0d7de2422"
                },
                "foliage_block_data_signature": "0x8c18eba47ec21495a23a585b0b01e6c5418c7e856640c3e0a914e108ddcf55e182e5f0e3830d06108b265f4095411ef40741f333ca7476171a85d4a5637b1e7ddb42018415df1235ebe4bff5d35511695e0af5e0d3a5e74b25db03805e5f621d",
                "foliage_transaction_block_hash": "0x4a8af14d06506534fa86c337b68716344f98ad6c5596a89ea943360e8c18ce01",
                "foliage_transaction_block_signature": "0xb9188feb37010a83038774a4dd6a727eb66256baf46baacaf9d83d0d986b3e563f2f87b161961ffe1e4e04d1399e03430254f1e584c591204e199e94cea5cff2c7ca10943f32706d8550b63ae0fd682005c37abe268cef3d87d94fdc0d6c0278",
                "prev_block_hash": "0x5a98b40a82040091846e703f7bef7e6c5ce4424b9dafbac76303dbf2c3bf0718",
                "reward_block_hash": "0xa61c2bfbfaade587c93f9136e899b0607a226df606a4ccc4c6d256ceb4747cd9"
                },
                "foliage_transaction_block": {
                "additions_root": "0xfceb313124f5f5c76224261abe7d2d7506c351d51e404c160439548d4fb1ac1b",
                "filter_hash": "0x63e57b682a28970108b4134988f6a43306eae086d46a3e6abb642dc3fb01bdfc",
                "prev_transaction_block_hash": "0x638e164bebfe63f4c467a707730718b54870c6b874d116336ab06d6feab5580f",
                "removals_root": "0x09084ac5ffbfc03369f2769629160923c9401d7d0227d1a58d90fa9013193b69",
                "timestamp": 1725991066,
                "transactions_info_hash": "0xf92d7a6f46076be61395e3b2aef394fef384992489633bd8112977a39847403f"
                },
                "infused_challenge_chain_ip_proof": {
                "normalized_to_identity": false,
                "witness": "0x0000a2bd898a5aa2af9a75675f56dcb9429fe3e1a9d68fe46238e890894949be14b452dfa4968525265eba5a35eaffebc950bbcdf34b7b040ccb53e2005d06bcec040ffcc4871d10be96aca520fb9f5b6e6aa4c4c4ad20a2d63a7de155761804f80a02000000000000781e00ebba70d004de0251bf6d3ee264fdacfc98c86122dfb43ededa850e501288cec5d701005e49a9b084a9764457143ff81e83da474bbf0e0172456167e6671b9935ab1fae0aff07d8a52639277af86ac808bc072adebad38d35994bfe55f004804bdd4426e9870080938dc55d5002e21b731df6c746bd07fa8657582ef321a946bf6f960601000000000001684a60e020aacfe2f0ce689f49d544fe14c92fc6fe03a9b76fd32533df44dbf2aa70190d0100391b54249dae3f289f7266b60bcd57ccde098633f507df872450589c7ede0c697818bd16def7b80d1f3f82c099719777b9f7202b47391e9e5503357f4ffd2d0c14d50ac866f5829f4ea3dbb17a2f5b3a638440d24437d9d7aed369f7f97dec060805",
                "witness_type": 2
                },
                "reward_chain_block": {
                "challenge_chain_ip_vdf": {
                    "challenge": "0x3b229fc43c8bc35db264000ac30309fdecffd27b353fcf88dd43d61f9d47a497",
                    "number_of_iterations": 288147196,
                    "output": {
                    "data": "0x010086aafeadf030ada0f9fd384c0c8802d3a157600e9d3a15dadf158d6bb1203f53173658e9439957b4b9111a945fdd1ccb8f72a3ce5cef7d03e1d370f0edef3d63b5c3ee46b7425dfd299d00a990dc7150bb046c06703a3d224aee2938153a92360100"
                    }
                },
                "challenge_chain_sp_signature": "0xb2611e18db89cd73f6f84601407e3462834fef77bbb74684eabd251bbf7defbe18b5926a8d88c866e784960d9998c2100631ff10afdddfa81a9480e176150ee74b88d99c97cd97094108de890fb9fd660acaace69eb7d1e01005f19c06ad6383",
                "challenge_chain_sp_vdf": {
                    "challenge": "0x3b229fc43c8bc35db264000ac30309fdecffd27b353fcf88dd43d61f9d47a497",
                    "number_of_iterations": 255066112,
                    "output": {
                    "data": "0x0200a4aec98980f7b1f87ab4fbc45b19425cb5c006ee104424d19b40ba016a1d3bfd0f0ad021b33e58f334e9dd1476d46ed27ce654929020d403a9d69ed659c3a8402531965bd9d43ef0b29781e97ff60db5d59e0a4aa5b8ca7292e935abada2bc150100"
                    }
                },
                "height": 5910291,
                "infused_challenge_chain_ip_vdf": {
                    "challenge": "0x5606978b68f9fe3db577ab132a772e485ef4a96a0a2a53f8ee7f77e627c9c5e8",
                    "number_of_iterations": 34225136,
                    "output": {
                    "data": "0x000007393fe8b0f177674aca1e287bc25818222308585ead5065b6b36c61fe051ef2ae1e45dd06e05c8cbf32d43d275a40d081203c4cbc3b581af52e3dae4e5dea0b0c86fc34324b44afe996936275d405af49d62f32e593bf9e97933c77de698c170200"
                    }
                },
                "is_transaction_block": true,
                "pos_ss_cc_challenge_hash": "0x3b229fc43c8bc35db264000ac30309fdecffd27b353fcf88dd43d61f9d47a497",
                "proof_of_space": {
                    "challenge": "0xf6204fc238ec82939ad48a34be3b9e73cf427ffa221c934d02a3fde8371e00f6",
                    "plot_public_key": "0x8dd0b25d7aeb522c6e3fd3bec90c232a992b3d12bbb06612e49f703375b0f4c316b1c724c6f084611e14f481e5c46a03",
                    "pool_contract_puzzle_hash": "0x3ac292ed271257be352c526f975a1b376752c4ed6e453ea39ed449bd7b6e3c24",
                    "pool_public_key": null,
                    "proof": "0xd61223e241ef0257ccc668099b52c211d17d3368d9bc424103dc35494c6fe155826c2a57a6af5417f5624e509f03572f2803bd04f66ae3d6bc6ff195c145cb18f5d2f67e8e89212a15509c8fc0f5dc4c866009327fe73112eaef9a2b0b5d069d3528f489714f3f3db0f7ad07e80ea0b81ac14913637bd12afa466bea3e8de02797c92e08e5dd148e9867849c92caebaaea7d60164e2f2a7d30bb11285100991e083bdb3c2de28c2b6bef21499287efb9f635eba2fb9722fd66c309c110ac46a5afd21081f3a8ada06a9b14d5c1369c3077ce6020f24473356d5070cbae959bc935f72d7516819209d595ccdf6dbe382f36b25c70faa8fa03c7a5fa56f4c51de0",
                    "size": 32
                },
                "reward_chain_ip_vdf": {
                    "challenge": "0xc11606b3b528b6e45a196ba79e4625a2956ff62c056e22c16c0f0dead1a454b9",
                    "number_of_iterations": 34225136,
                    "output": {
                    "data": "0x0200d0abae11ee4e327bb67ba5baee204da48f23453099cb1ff7c04a7c7b08d03980d01e0ffe7263e5810a11ec99a218ff089165902192de7e4ce6ee8854b67bd489b96cfc124cc0e85c29e9fa23e4debccf59bd6a29415e2a321a219e0bd9cb8f3a0100"
                    }
                },
                "reward_chain_sp_signature": "0xa75ec769ecc4109317c8f9be6cf879e076ffbe23645f17482d2e3bcd5ae4ee05365d5a9e36d8b30895cc12494b75b2e20dc18bf343861f81a72581799e36ca9b11c681e2ee48260b9557a5a4930cde62fc494a8085e35696eacbb021392c3a4d",
                "reward_chain_sp_vdf": {
                    "challenge": "0xc11606b3b528b6e45a196ba79e4625a2956ff62c056e22c16c0f0dead1a454b9",
                    "number_of_iterations": 1144052,
                    "output": {
                    "data": "0x02002623e30e6805d06247ca15976c75c541531329f69627c36631ec53c5661de771965e9894d1fe9075133d716e66eba1b4a00c34a0f2ef58fc149c5406bd296b33e1cf44be99ae72160e75b1c99fed284a68507fd0191c620bb37479cea4526c020200"
                    }
                },
                "signage_point_index": 28,
                "total_iters": 45503147198204,
                "weight": 27802025488
                },
                "reward_chain_ip_proof": {
                "normalized_to_identity": false,
                "witness": "0x01007eea0de3c013b51b6c4ba9a878efc51b93ff0173c5d358171f44e02eae59d6859243acbd1105456de342b710f20cb8fadc3af785aa0757c5f87e171dabb918275b2ded625ad5bb8f6c34c78011d0bf928573d479764f47385f859b3b3cf25e250100000000000016f300c81c899285ec7a40ca7a592e224a35de7b4a9f11e2163c8640be948f786572b621020078009e53ff37116fb1cc8e30ee646beb8e7a804a6592fd564e7b36f8fe234916a6d127535a1fa530fc99c0fdb96adbe99c734710763418400f6a402b952359027b45f3b2a9b2c86dd548292b6e12a5217d755e445759caa88d4bb411c1898c00130f000000000044e8a0b68edd518dae1b1a99ff80fa71282bcd0ad45a31876b3371c08c41c522c06487cd020018d5230eb62037be756e0a48487916ce84ea8d92b82bfae3c46bcfb64ff3afce96148274c39edae96a7bb75ab2fad437133ffddc004452dd321cb6505c02608bbdb993f12d4cd48b04bf05a1da25fbd7f0cd65e7521e2a1b56ab89d78cab586d0100000000000022ca40847fb4e0a3784d972bf6499b9602845d314a9794b830d2b206a627bdd29c9f9a1902001bd634a7210d0ee0d9858cdf6c55820291824c2d6c0685e3dad98c1e5aecba3f9a54ae2aedad61faa0d0dc22ff1b5147ccbc13031549adbacca8935af5311210d126fafaa192b20fd6ce521d1f37d5f26be455dac591f76bb37ce1b13eef581d02000000000000682fe0dc8b346cf205fa7fd21e19d988cd029d4a275546982e0a6f0194d64459952862ef0000b8a38ed48a241153b49a04784b4e8e5baf512b02744865b80d769196567aa2203c23eb19d476d7dfae27757de0d10aae45691098f6d6db8da1775ebeabb35d08ad357c37586dd7fde12eaddec7e5536d0548fbe6d0f56b30f6807eab6ed8690508020000000000229b608dae620f251cbdaa5afcdf6730ce465e0cc09926b5ad463b138dc648a48858a1c70100e0e746c2ed703a896f47c78aaa8c20545d0fa5f1588ad477e6b2027f7e9d1c6f25ff8abf765275700e535b1d1442440b2cc525de0ed6d1c127605cb05a68d78e25ad61b026475e104f266f061aedd640c08cbfa603833b46c5532da1510af2650100000000000067d220857041d223eb090f7b31a3dfc1c66accfdda2664131e1cb38e5e126605d0f21fbf0100e48406311dbf8054199fbc47dc5f68cc7c2c71271752f1231e7f79e71c33e1f2029a52e2209977106c3726900859df55230b9cf4627c3aaf947fe5e79ba6016ed10fb143b7f6608b60827c702b23927edccc9833db71994ed329d658a882cb340100000000000020f580e02adb6f55e8df89300d1f4eed9e2759a3ef7c5bfcec8606ccb4d0d7b24522434b0000e4492d9e872c6d39d8e5c2fa12cee3996dc0962ff5a4f5decea3f332c40a2680ce0430ac0547d99997d49b47a5ecf21b096b6a3e6eaa57c51fee52fcd89f1b728d83b6375024c1b00c2918ed71ddcbc362b9950c9573fe9f35747c9b7a434c3f0100000000000062f020ade99b5112bb363da39d85842ddaedaf9ccade1dc3f3f6ca82fa11f15c545b69e3000058285dda67599dd9e33cb468fbce1ac84ee403fd211b68e9f51f737be25172e24e433ff5b1d166e0903fb0692fecc9a57161a5bbeaa470d432b6d29af785cd23859aaea7a41fe774518fc15efa0641e2a6a2a451f6fbc735c51834ee1fcfcf32010000000000000d0fc0e13fdaaf42287e3b4f74a4b69e6087022e3ab1e596f209be7b68159a2f0978351101001595f7da758e3006a16985ccd8867b18b2e840d32ea2eda2398ef0370e150261cef263c1ff5f17a333b747432991f6a2396769b46747620e7365fa88f823bc3735f9513ebc6ae83d0cf04027ad09cef12fbca7a8a8b162127f741f3abe6f89070100",
                "witness_type": 9
                },
                "reward_chain_sp_proof": {
                "normalized_to_identity": false,
                "witness": "0x02000db29d4f8513d0cb7c56162478a539ff5b863608514b52dd2ee9cee690b5deec82eafca531e72e2da4473bc0fd616b0b6435217ae0fd4b9fa2f2c6c805e7b215556d9d7ec4db0d9c99a2d9c8ced4879fc9855d01cd68b1c38e105a52f968d822020000000000000d0fc0e13fdaaf42287e3b4f74a4b69e6087022e3ab1e596f209be7b68159a2f0978351101001595f7da758e3006a16985ccd8867b18b2e840d32ea2eda2398ef0370e150261cef263c1ff5f17a333b747432991f6a2396769b46747620e7365fa88f823bc3735f9513ebc6ae83d0cf04027ad09cef12fbca7a8a8b162127f741f3abe6f89070100",
                "witness_type": 1
                },
                "transactions_generator": "0x80",
                "transactions_generator_ref_list": [],
                "transactions_info": {
                "aggregated_signature": "0xa6ffd8e8b1df9d35e11d2dbd29b81827f543963d817c5026619a44a31a8558823a0c18b0b533bca2b68dfcb5000d759200b5e3648420dc108ddd2a455f8a966d2f593d728b7858915e2c5600f8d030354b37a556bb2e3bef9ec7a6af9d15494f",
                "cost": 162600245,
                "fees": 205449550,
                "generator_refs_root": "0x0101010101010101010101010101010101010101010101010101010101010101",
                "generator_root": "0x6b2c8956c7d31a0ed3a4440cae16f2b7ee6917818909f052867f08e407d3d589",
                "reward_claims_incorporated": [
                    {
                    "amount": 875000000000,
                    "parent_coin_info": "0xccd5bb71183532bff220ba46c268991a000000000000000000000000005a2f10",
                    "puzzle_hash": "0x3a9ba81f693aac5ba77bc1e42ff55fda4cbaab478bdb1302b5fbe64398f272cd"
                    },
                    {
                    "amount": 125000000000,
                    "parent_coin_info": "0x3ff07eb358e8255a65c30a2dce0e5fbb000000000000000000000000005a2f10",
                    "puzzle_hash": "0xf120223400081b4b58b83eafdbfa536b306c055c56ee6491824e91cc05f88845"
                    }
                ]
                }
            },
            "success": true
            }"#,
        );

        let response = client
            .get_block(Bytes32::from(hex!(
                "88a8e404c419e12bb11e809ff7afc8b1fcda77270fe3f157cff8a2fab4f44e8b"
            )))
            .await
            .unwrap();

        assert!(response.success);
        assert!(response.block.is_some());
        assert!(response.error.is_none());

        let block = response.block.unwrap();
        assert_eq!(block.height(), 5_910_291);
        assert_eq!(block.weight(), 27_802_025_488);
        assert_eq!(block.total_iters(), 45_503_147_198_204);

        assert!(block.transactions_info.is_some());
        assert!(
            block
                .transactions_info
                .unwrap()
                .reward_claims_incorporated
                .len()
                == 2
        );
    }

    #[tokio::test]
    async fn test_get_block_record_success() {
        let mut client = MockRpcClient::new();

        client.mock_response(
            "http://api.example.com/get_block_record",
            r#"{
                "block_record": {
                    "challenge_block_info_hash": "0xc3a285c97ef0fd5b941e2133432159ff6db2599e2f806cb3565781ccf6427689",
                    "challenge_vdf_output": {
                    "data": "0x010086aafeadf030ada0f9fd384c0c8802d3a157600e9d3a15dadf158d6bb1203f53173658e9439957b4b9111a945fdd1ccb8f72a3ce5cef7d03e1d370f0edef3d63b5c3ee46b7425dfd299d00a990dc7150bb046c06703a3d224aee2938153a92360100"
                    },
                    "deficit": 3,
                    "farmer_puzzle_hash": "0xd86028d22a28f4d0e4ee63808492630e7829af653fd710c683980959bb7bba1d",
                    "fees": 205449550,
                    "finished_challenge_slot_hashes": null,
                    "finished_infused_challenge_slot_hashes": null,
                    "finished_reward_slot_hashes": null,
                    "header_hash": "0x88a8e404c419e12bb11e809ff7afc8b1fcda77270fe3f157cff8a2fab4f44e8b",
                    "height": 5910291,
                    "infused_challenge_vdf_output": {
                    "data": "0x000007393fe8b0f177674aca1e287bc25818222308585ead5065b6b36c61fe051ef2ae1e45dd06e05c8cbf32d43d275a40d081203c4cbc3b581af52e3dae4e5dea0b0c86fc34324b44afe996936275d405af49d62f32e593bf9e97933c77de698c170200"
                    },
                    "overflow": false,
                    "pool_puzzle_hash": "0x3ac292ed271257be352c526f975a1b376752c4ed6e453ea39ed449bd7b6e3c24",
                    "prev_hash": "0x5a98b40a82040091846e703f7bef7e6c5ce4424b9dafbac76303dbf2c3bf0718",
                    "prev_transaction_block_hash": "0x638e164bebfe63f4c467a707730718b54870c6b874d116336ab06d6feab5580f",
                    "prev_transaction_block_height": 5910288,
                    "required_iters": 5752572,
                    "reward_claims_incorporated": [
                    {
                        "amount": 875000000000,
                        "parent_coin_info": "0xccd5bb71183532bff220ba46c268991a000000000000000000000000005a2f10",
                        "puzzle_hash": "0x3a9ba81f693aac5ba77bc1e42ff55fda4cbaab478bdb1302b5fbe64398f272cd"
                    },
                    {
                        "amount": 125000000000,
                        "parent_coin_info": "0x3ff07eb358e8255a65c30a2dce0e5fbb000000000000000000000000005a2f10",
                        "puzzle_hash": "0xf120223400081b4b58b83eafdbfa536b306c055c56ee6491824e91cc05f88845"
                    },
                    {
                        "amount": 875000000000,
                        "parent_coin_info": "0xccd5bb71183532bff220ba46c268991a000000000000000000000000005a2f0f",
                        "puzzle_hash": "0x3a9ba81f693aac5ba77bc1e42ff55fda4cbaab478bdb1302b5fbe64398f272cd"
                    },
                    {
                        "amount": 125000000000,
                        "parent_coin_info": "0x3ff07eb358e8255a65c30a2dce0e5fbb000000000000000000000000005a2f0f",
                        "puzzle_hash": "0xf120223400081b4b58b83eafdbfa536b306c055c56ee6491824e91cc05f88845"
                    },
                    {
                        "amount": 875000000000,
                        "parent_coin_info": "0xccd5bb71183532bff220ba46c268991a000000000000000000000000005a2f0e",
                        "puzzle_hash": "0xcd975114b70a116841c7abf214d6c859f1ad5b11b95bb2cfa5a7dfa6883e805f"
                    },
                    {
                        "amount": 125000000000,
                        "parent_coin_info": "0x3ff07eb358e8255a65c30a2dce0e5fbb000000000000000000000000005a2f0e",
                        "puzzle_hash": "0x480454e7d65a9cd6a71944953bcc32ad622b23184091579a1318446fa7301a2d"
                    }
                    ],
                    "reward_infusion_new_challenge": "0xa61c2bfbfaade587c93f9136e899b0607a226df606a4ccc4c6d256ceb4747cd9",
                    "signage_point_index": 28,
                    "sub_epoch_summary_included": null,
                    "sub_slot_iters": 583008256,
                    "timestamp": 1725991066,
                    "total_iters": 45503147198204,
                    "weight": 27802025488
                },
                "success": true
            }"#,
        );

        let response = client
            .get_block_record(Bytes32::from(hex!(
                "88a8e404c419e12bb11e809ff7afc8b1fcda77270fe3f157cff8a2fab4f44e8b"
            )))
            .await
            .unwrap();

        assert!(response.success);
        assert!(response.block_record.is_some());
        assert!(response.error.is_none());

        let block_record = response.block_record.unwrap();
        assert_eq!(block_record.height, 5_910_291);
        assert_eq!(block_record.weight, 27_802_025_488);
        assert_eq!(block_record.total_iters, 45_503_147_198_204);
        assert_eq!(block_record.reward_claims_incorporated.unwrap().len(), 6);
    }

    #[tokio::test]
    async fn test_get_puzzle_and_solution() {
        let mut client = MockRpcClient::new();

        client.mock_response(
            "http://api.example.com/get_puzzle_and_solution",
            r#"{
                "coin_solution": {
                    "coin": {
                    "amount": 7100000,
                    "parent_coin_info": "0xa7658d2add3c2fc83eb07cc2655e4fe4fe630627dd07d3e9a83f9dac03ada8b1",
                    "puzzle_hash": "0xfbacdd2364a53e036af892c9d4e3b593eb12ae4a939776a251abb68de857b3fc"
                    },
                    "puzzle_reveal": "0xff04ff02ff8080",
                    "solution": "0xff0180"
                },
                "success": true
            }"#,
        );

        let coin_id = Bytes32::from(hex!(
            "bae7ca992d0062ec050158d00880577afc38a12b5c3af1874d2ce2759eb50ae1"
        ));
        let response = client.get_puzzle_and_solution(coin_id, None).await.unwrap();
        assert!(response.success);
        assert!(response.coin_solution.is_some());
        assert!(response.error.is_none());

        let coin_solution = response.coin_solution.unwrap();
        assert_eq!(coin_solution.coin.coin_id(), coin_id);
        assert_eq!(
            coin_solution.puzzle_reveal.to_bytes().unwrap(),
            hex!("ff04ff02ff8080")
        );
        assert_eq!(coin_solution.solution.to_bytes().unwrap(), hex!("ff0180"));
    }
}
