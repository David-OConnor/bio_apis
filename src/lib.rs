use std::{io, time::Duration};

use ureq::Agent;

pub mod drugbank;
pub mod ncbi;
pub mod pubchem;
pub mod rcsb;

// Workraound for not being able to construct ureq's errors.
pub struct ReqError {}

impl From<ureq::Error> for ReqError {
    fn from(_err: ureq::Error) -> Self {
        Self {}
    }
}

impl From<io::Error> for ReqError {
    fn from(_err: io::Error) -> Self {
        Self {}
    }
}

const HTTP_TIMEOUT: u64 = 3; // In seconds

fn make_agent() -> Agent {
    let config = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(HTTP_TIMEOUT)))
        .build();

    config.into()
}
