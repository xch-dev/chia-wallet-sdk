use std::sync::{Arc, Mutex};

use crate::FullNodeSimulator;

pub type SharedSimulator = Arc<Mutex<FullNodeSimulator>>;
