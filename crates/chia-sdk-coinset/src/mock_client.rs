use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Mutex;

use crate::ChiaRpcClient;

#[derive(Debug)]
pub struct MockRpcClient {
    requests: Mutex<Vec<(String, Value)>>,
    responses: HashMap<String, String>,
}

impl MockRpcClient {
    pub fn new() -> Self {
        Self {
            requests: Mutex::new(Vec::new()),
            responses: HashMap::new(),
        }
    }

    pub fn mock_response(&mut self, url: &str, response: &str) {
        self.responses.insert(url.to_string(), response.to_string());
    }

    pub fn get_requests(&self) -> Vec<(String, Value)> {
        self.requests.lock().unwrap().clone()
    }

    pub fn post(&self, url: &str, json: Value) -> Result<String, Box<dyn Error>> {
        self.requests.lock().unwrap().push((url.to_string(), json));

        match self.responses.get(url) {
            Some(response) => Ok(response.clone()),
            None => Err("No mock response configured for URL".into()),
        }
    }
}

impl Default for MockRpcClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ChiaRpcClient for MockRpcClient {
    type Error = Box<dyn Error>;

    fn base_url(&self) -> &'static str {
        "http://api.example.com"
    }

    async fn make_post_request<R, B>(&self, endpoint: &str, body: B) -> Result<R, Self::Error>
    where
        B: Serialize,
        R: DeserializeOwned,
    {
        let url = format!("{}/{}", self.base_url(), endpoint);
        let body = serde_json::to_value(body)?;
        let response = self.post(&url, body)?;
        Ok(serde_json::from_str::<R>(&response)?)
    }
}
