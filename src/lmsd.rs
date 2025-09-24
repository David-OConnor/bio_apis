//! [Home page](https://www.lipidmaps.org/lmsd_search/30105))

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://www.lipidmaps.org/databases/lmsd";

pub fn open_overview(ident: &str) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/{}", ident.to_uppercase())) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Note: This is a 2D SDF.
fn sdf_url(ident: &str) -> String {
    format!("{BASE_URL}/{}?format=sdf", ident.to_uppercase())
}

/// Download an SDF file from DrugBank, returning an SDF string.
pub fn load_sdf(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    Ok(agent
        .get(sdf_url(ident))
        .call()?
        .body_mut()
        .read_to_string()?)
}
