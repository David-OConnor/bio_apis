//! [Home page](https://www.ebi.ac.uk/pdbe/)
//! [API docs](https://www.ebi.ac.uk/pdbe/api/)

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://www.ebi.ac.uk/pdbe-srv/pdbechem/chemicalCompound/show";

pub fn open_overview(id: &str) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/{id}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

// /// Find proteins associated with this small organic molecule, e.g. if it's a ligand,
// /// which proteins it can bind to. This notably includes PDB urls
// pub fn load_associated_structures(ident_pubchem: u32) -> Result<Vec<ProteinStructure>, ReqError> {
//     let url = format!("{PROTEIN_LOOKUP_URL}/{ident_pubchem}/JSON");
//     let agent = make_agent();
//
//     let resp = agent.get(url).call()?.body_mut().read_to_string()?;
//
//     let parsed: ProteinStructureResponse = serde_json::from_str(&resp)?;
//     Ok(parsed.structure.structures)
// }

/// Note: This loads the "ideal" SDF; not the "model" one.
fn sdf_url(ident: &str) -> String {
    format!(
        "https://www.ebi.ac.uk/pdbe/static/files/pdbechem_v2/{}_ideal.sdf",
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
