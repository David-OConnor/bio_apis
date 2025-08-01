//! [API docs](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest)

use serde::Deserialize;

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://pubchem.ncbi.nlm.nih.gov/compound";
const PROTEIN_LOOKUP_URL: &str =
    "https://pubchem.ncbi.nlm.nih.gov/rest/pug_view/structure/compound";

#[derive(Clone, Debug, Deserialize)]
pub struct Taxonomy {
    #[serde(rename = "ID")]
    id: u32,
    #[serde(rename = "Name")]
    name: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct ProteinStructure {
    #[serde(rename = "MMDB_ID")]
    pub mmdb_id: u32,
    #[serde(rename = "PDB_ID")]
    pub pdb_id: String,
    #[serde(rename = "URL")]
    pub url: String,
    #[serde(rename = "ImageURL")]
    pub image_url: String,
    #[serde(rename = "Description")]
    pub description: String,
    #[serde(rename = "Taxonomy")]
    pub taxonomy: Taxonomy,
}

#[derive(Deserialize)]
struct InnerStructure {
    #[serde(rename = "Structures")]
    structures: Vec<ProteinStructure>,
}

#[derive(Deserialize)]
struct ProteinStructureResponse {
    #[serde(rename = "Structure")]
    structure: InnerStructure,
}

pub fn open_overview(id: u32) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/{id}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Find proteins associated with this small organic molecule, e.g. if it's a ligand,
/// which proteins it can bind to. This notably includes PDB urls
pub fn load_associated_structures(ident_pubchem: u32) -> Result<Vec<ProteinStructure>, ReqError> {
    let url = format!("{PROTEIN_LOOKUP_URL}/{ident_pubchem}/JSON");
    let agent = make_agent();

    let resp = agent.get(url).call()?.body_mut().read_to_string()?;

    let parsed: ProteinStructureResponse = serde_json::from_str(&resp)?;
    Ok(parsed.structure.structures)
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
