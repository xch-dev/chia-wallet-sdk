use serde_json::Value;
use std::collections::HashMap;
use std::error::Error;
use std::sync::Mutex;

#[derive(Debug)]
pub struct MockChiaClient {
    requests: Mutex<Vec<(String, Value)>>,
    responses: HashMap<String, String>,
}

impl MockChiaClient {
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

    pub async fn post(&self, url: &str, json: Value) -> Result<String, Box<dyn Error>> {
        self.requests.lock().unwrap().push((url.to_string(), json));

        match self.responses.get(url) {
            Some(response) => Ok(response.clone()),
            None => Err("No mock response configured for URL".into()),
        }
    }
}

impl Default for MockChiaClient {
    fn default() -> Self {
        Self::new()
    }
}
