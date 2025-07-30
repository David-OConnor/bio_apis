use std::{io, time::Duration};

use ureq::Agent;

pub mod amber_geostd;
pub mod drugbank;
pub mod ncbi;
pub mod pubchem;
pub mod rcsb;

// Workraound for not being able to construct ureq's errors.
#[derive(Debug)]
pub enum ReqError {
    Http,
    Ser(serde_json::Error),
    Io(io::Error),
}

impl From<ureq::Error> for ReqError {
    fn from(_err: ureq::Error) -> Self {
        Self::Http
    }
}

impl From<serde_json::Error> for ReqError {
    fn from(err: serde_json::Error) -> Self {
        Self::Ser(err)
    }
}

impl From<io::Error> for ReqError {
    fn from(err: io::Error) -> Self {
        Self::Io(err)
    }
}

const HTTP_TIMEOUT: u64 = 5; // In seconds

fn make_agent() -> Agent {
    let config = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(HTTP_TIMEOUT)))
        // Don't cause 404 and similar error HTTP codes to throw errors when making HTTP requests.
        .http_status_as_error(false)
        .build();

    config.into()
}
