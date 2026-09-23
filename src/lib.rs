use std::{io, time::Duration};

use ureq::Agent;

pub mod amber_geostd;
pub mod bmrb;
pub mod brenda;
pub mod chebi;
pub mod drugbank;
pub mod emdb;
pub mod lmsd;
pub mod mcsa;
pub mod ncbi;
pub mod pdbe;
pub mod pubchem;
pub mod rcsb;
pub mod rhea;
pub mod uniprot;

// Workraound for not being able to construct ureq's errors.
#[derive(Debug)]
pub enum ReqError {
    Http,
    Ser(serde_json::Error),
    Deserialize,
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
    make_agent_with_timeout(HTTP_TIMEOUT)
}

/// For endpoints known to be slower than our default timeout allows, e.g. structure searches.
fn make_agent_with_timeout(timeout: u64) -> Agent {
    let config = Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(timeout)))
        // Don't cause 404 and similar error HTTP codes to throw errors when making HTTP requests.
        .http_status_as_error(false)
        .build();

    config.into()
}
