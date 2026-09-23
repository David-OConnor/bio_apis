//! [Home page](https://www.ebi.ac.uk/pdbe/)
//! [API docs](https://www.ebi.ac.uk/pdbe/api/)

use std::collections::HashMap;

use serde::Deserialize;

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://www.ebi.ac.uk/pdbe-srv/pdbechem/chemicalCompound/show";
const ENTRY_URL: &str = "https://www.ebi.ac.uk/pdbe/entry/pdb";
const MAPPINGS_URL: &str = "https://www.ebi.ac.uk/pdbe/api/mappings";
const ENTRY_FILES_URL: &str = "https://www.ebi.ac.uk/pdbe/entry-files/download";

// ---- Best structures (UniProt -> PDB) --------------------------------------

/// One experimental structure containing a UniProt entry, from PDBe's "best structures" ranking.
/// There is one of these per (structure, chain) pair.
#[derive(Clone, Debug, Deserialize)]
pub struct BestStructure {
    /// Lowercase, e.g. `"1jms"`. Pass to `load_cif` here, or `rcsb::load_cif`.
    pub pdb_id: String,
    /// The chain of the structure the UniProt sequence maps to, e.g. `"A"`.
    pub chain_id: String,
    /// E.g. `"X-ray diffraction"`, `"Electron Microscopy"`, `"Solution NMR"`.
    pub experimental_method: Option<String>,
    /// In Å. Absent for methods that don't report one, e.g. NMR.
    pub resolution: Option<f32>,
    /// The NCBI taxonomy identifier of the source organism.
    pub tax_id: Option<u32>,
    /// First residue this chain covers, in the **UniProt** sequence (1-based).
    pub unp_start: u32,
    /// Last residue this chain covers, in the **UniProt** sequence (1-based).
    pub unp_end: u32,
    /// Fraction of the UniProt sequence this chain covers (0–1).
    pub coverage: f32,
}

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

/// Our agent doesn't treat error status codes as errors; catch them here, so we don't hand an error
/// page back to the caller as if it were data.
fn get(url: &str) -> Result<String, ReqError> {
    let agent = make_agent();
    let mut resp = agent.get(url).call()?;

    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    Ok(resp.body_mut().read_to_string()?)
}

/// PDBe's entry-file endpoints only take the classic 4-character ID; RCSB's extended form, e.g.
/// `pdb_00001crn`, maps onto it by dropping the prefix and leading zeros.
fn bare_pdb_id(pdb_id: &str) -> String {
    let id = pdb_id.trim().to_lowercase();

    match id.strip_prefix("pdb_") {
        Some(ext) => {
            let short = ext.trim_start_matches('0');
            match short.len() == 4 {
                true => short.to_owned(),
                false => ext.to_owned(),
            }
        }
        None => id,
    }
}

/// Experimental structures containing a UniProt entry, ranked by PDBe: by how much of the UniProt
/// sequence they cover, then by resolution. The first entry is generally the best representative
/// structure of the protein. There is one item per (structure, chain) pair, so a PDB ID can appear
/// more than once.
///
/// Returns an empty `Vec` if the protein has no experimental structures.
///
/// API: `https://www.ebi.ac.uk/pdbe/api/mappings/best_structures/{accession}`
pub fn load_best_structures(accession: &str) -> Result<Vec<BestStructure>, ReqError> {
    let accession = crate::uniprot::parse_accession(accession);
    let url = format!("{MAPPINGS_URL}/best_structures/{accession}");

    let agent = make_agent();
    let mut resp = agent.get(&url).call()?;

    // PDBe responds 404, with a message body, for an accession it has no structures of.
    if resp.status() == 404 {
        return Ok(Vec::new());
    }
    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    // Keyed by the accession.
    let mut raw: HashMap<String, Vec<BestStructure>> =
        serde_json::from_str(&resp.body_mut().read_to_string()?)?;

    Ok(raw.drain().next().map(|(_, v)| v).unwrap_or_default())
}

/// The PDB IDs of `load_best_structures`, deduplicated, in rank order. E.g. `["1jms", "4i2a"]`.
pub fn best_pdb_ids(accession: &str) -> Result<Vec<String>, ReqError> {
    let mut result: Vec<String> = Vec::new();

    for s in load_best_structures(accession)? {
        if !result.contains(&s.pdb_id) {
            result.push(s.pdb_id);
        }
    }

    Ok(result)
}

/// Download an entry's (atomic coordinates) mmCIF file from PDBe, returning a CIF string. This is
/// the same structure RCSB serves for the ID. Accepts the 4-character ID, e.g. `1crn`, or the
/// extended one, e.g. `pdb_00001crn`.
pub fn load_cif(pdb_id: &str) -> Result<String, ReqError> {
    get(&format!("{ENTRY_FILES_URL}/{}.cif", bare_pdb_id(pdb_id)))
}

/// Open the page for a chemical component, e.g. a ligand, given its PDBe ID, e.g. `ATP`.
pub fn open_overview(id: &str) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/{id}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Open the page for a structure, e.g. a protein, given its PDB ID, e.g. `1crn`.
pub fn open_entry(pdb_id: &str) {
    if let Err(e) = webbrowser::open(&format!("{ENTRY_URL}/{}", pdb_id.to_lowercase())) {
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

/// Download a chemical component's SDF file from PDBe, e.g. for `ATP`, returning a SDF string.
pub fn load_sdf(ident: &str) -> Result<String, ReqError> {
    get(&sdf_url(ident.trim()))
}
