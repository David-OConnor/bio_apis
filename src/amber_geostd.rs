//! Load amber Mol2, lib, and FRCMOD data for a given small organic molecule.
//! These are from the AMBER_GEOSTD collection, CAO Amber 2025, and hosted
//! on our own system.
//!
//! Note: Identifiers in this module can be Amber GeoStd/PDBe (Should be same), or PubChem.

use std::collections::HashMap;

use serde::Deserialize;

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://www.athanorlab.com";

#[derive(Clone, Debug, Deserialize)]
pub struct GeostdItem {
    pub ident: String,
    pub frcmod_avail: bool,
    pub lib_avail: bool,
}

/// Contains the text content of these files.
#[derive(Clone, Debug, Deserialize)]
pub struct GeostdData {
    pub mol2: String,
    pub frcmod: Option<String>,
    pub lib: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
struct GeostdItemResponse {
    pub result: Vec<GeostdItem>,
}

/// Get a list of all molecules available from Amber Geostd, and if they include FRCMOD and lib data.
pub fn get_all_mols() -> Result<Vec<GeostdItem>, ReqError> {
    let agent = make_agent();

    let url = format!("{BASE_URL}/get-all-mols");
    let resp = agent.get(url).call()?.body_mut().read_to_string()?;

    let parsed: GeostdItemResponse = serde_json::from_str(&resp)?;
    Ok(parsed.result)
}

/// Search for molecules by keyword, and find if they include FRCMOD and lib data.
pub fn find_mols(search_text: &str) -> Result<Vec<GeostdItem>, ReqError> {
    let agent = make_agent();

    let mut params = HashMap::new();
    params.insert("search_text", search_text);
    let payload_json = serde_json::to_string(&params)?;

    let url = format!("{BASE_URL}/find-mols");

    let resp = agent
        .post(url)
        .header("Content-Type", "application/json")
        .send(&payload_json)?
        .body_mut()
        .read_to_string()?;

    let parsed: GeostdItemResponse = serde_json::from_str(&resp)?;
    Ok(parsed.result)
}

/// Download a Mol2 file's text, and if available, FRCMOD and Lib.
pub fn load_mol_files(ident: &str) -> Result<GeostdData, ReqError> {
    let agent = make_agent();

    let mut params = HashMap::new();
    params.insert("ident", ident);
    let payload_json = serde_json::to_string(&params)?;

    let url = format!("{BASE_URL}/load-mol-files");

    let resp = agent
        .post(url)
        .header("Content-Type", "application/json")
        .send(&payload_json)?
        .body_mut()
        .read_to_string()?;

    Ok(serde_json::from_str(&resp)?)
}
