//! [API docs](https://docs.drugbank.com/v1/)

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://go.drugbank.com/drugs";

pub fn open_overview(id: &str) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/{id}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

fn sdf_url(ident: &str) -> String {
    format!(
        "https://go.drugbank.com/structures/small_molecule_drugs/{}.sdf?type=3d",
        ident.to_uppercase()
    )
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
