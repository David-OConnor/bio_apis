//! [API docs](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest)

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://pubchem.ncbi.nlm.nih.gov/compound";

pub fn open_overview(id: u32) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/{id}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

fn sdf_url(ident: &str) -> String {
    // todo: LIkely wrong.
    format!(
        "https://pubchem.ncbi.nlm.nih.gov/rest/pug/conformers/0000FE0400000001/SDF?response_type=\
        save&response_basename=Conformer3D_COMPOUND_CID_{}",
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
