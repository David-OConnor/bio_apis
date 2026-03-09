//! [Home page](https://www.ebi.ac.uk/pdbe/)
//! [API docs](https://www.ebi.ac.uk/pdbe/api/)

use std::collections::HashMap;

use serde::Deserialize;

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://www.ebi.ac.uk/pdbe-srv/pdbechem/chemicalCompound/show";
const MAPPINGS_URL: &str = "https://www.ebi.ac.uk/pdbe/api/mappings";

// ---- SIFTS / UniProt mapping types ----------------------------------------

/// A residue position in the PDB structure, from a SIFTS mapping.
#[derive(Clone, Debug, Deserialize)]
pub struct SiftsResiduePosition {
    /// Sequential (1-based) residue number in the PDB chain.
    pub residue_number: i32,
    /// Author-assigned residue number (may differ from sequential, and may be
    /// absent for engineered residues).
    pub author_residue_number: Option<i32>,
    /// Insertion code used by some legacy PDB entries (e.g. `"A"`).
    #[serde(default)]
    pub author_insertion_code: Option<String>,
}

/// One contiguous segment of a UniProt sequence mapped onto a PDB chain.
#[derive(Clone, Debug, Deserialize)]
pub struct SiftsMapping {
    pub entity_id: u32,
    /// PDB chain identifier (author label), e.g. `"A"`.
    pub chain_id: String,
    /// Internal asymmetric-unit chain ID used in mmCIF files.
    pub struct_asym_id: String,
    /// First residue of this segment in the **UniProt** sequence (1-based).
    pub unp_start: u32,
    /// Last residue of this segment in the **UniProt** sequence (1-based).
    pub unp_end: u32,
    /// First residue of this segment in the **PDB** structure.
    pub start: SiftsResiduePosition,
    /// Last residue of this segment in the **PDB** structure.
    pub end: SiftsResiduePosition,
    /// Sequence identity between the PDB chain and the UniProt sequence (0–1).
    pub identity: f32,
    /// Fraction of the UniProt sequence covered by this structure (0–1).
    pub coverage: f32,
}

/// All SIFTS mappings for one UniProt entry within a PDB structure.
#[derive(Clone, Debug)]
pub struct SiftsUniprotMapping {
    /// UniProt accession code, e.g. `"P29373"`.
    pub accession: String,
    /// UniProt entry name, e.g. `"RABP2_HUMAN"`.
    pub identifier: String,
    /// Contiguous chain segments that map this UniProt sequence onto the structure.
    pub mappings: Vec<SiftsMapping>,
}

// Private serde helpers — the API nests data under dynamic PDB-ID and accession keys.
#[derive(Deserialize)]
struct RawUniprotEntry {
    identifier: String,
    mappings: Vec<SiftsMapping>,
}

#[derive(Deserialize)]
struct RawUniprotSection {
    #[serde(rename = "UniProt")]
    uniprot: HashMap<String, RawUniprotEntry>,
}

// ---------------------------------------------------------------------------

/// Fetch SIFTS UniProt–PDB residue-level mappings for a given PDB entry.
///
/// Returns one [`SiftsUniprotMapping`] per UniProt accession present in the
/// structure. Each entry carries the chain segments linking UniProt sequence
/// positions to PDB residue numbers — everything needed to color-code chains
/// by their UniProt identity in a visualizer like Molchanica.
///
/// API: `https://www.ebi.ac.uk/pdbe/api/mappings/uniprot/{pdb_id}`
pub fn load_uniprot_mappings(pdb_id: &str) -> Result<Vec<SiftsUniprotMapping>, ReqError> {
    let url = format!("{MAPPINGS_URL}/uniprot/{}", pdb_id.to_lowercase());
    let agent = make_agent();

    let resp = agent.get(&url).call()?.body_mut().read_to_string()?;

    // Top-level key is the (lowercased) PDB ID; take whichever entry is present.
    let mut raw: HashMap<String, RawUniprotSection> = serde_json::from_str(&resp)?;
    let section = raw.drain().next().ok_or(ReqError::Deserialize)?.1;

    Ok(section
        .uniprot
        .into_iter()
        .map(|(accession, entry)| SiftsUniprotMapping {
            accession,
            identifier: entry.identifier,
            mappings: entry.mappings,
        })
        .collect())
}

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

/// Download an SDF file from PDBe, returning a SDF string.
pub fn load_sdf(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    Ok(agent
        .get(sdf_url(ident))
        .call()?
        .body_mut()
        .read_to_string()?)
}
