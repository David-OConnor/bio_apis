//! [Home page](https://www.ebi.ac.uk/thornton-srv/m-csa/)
//! [API and download documentation](https://www.ebi.ac.uk/thornton-srv/m-csa/download/)
//!
//! M-CSA (the Mechanism and Catalytic Site Atlas) links enzyme sequences and structures to
//! catalytic residues, cofactors, overall reactions, and step-by-step chemical mechanisms. This
//! makes it useful after a reaction search in Rhea or BRENDA: M-CSA can identify the active-site
//! geometry and residue roles that an enzyme-design campaign needs to preserve.
//!
//! The public API has two supported views. `entries` contains enzyme- and mechanism-level data;
//! `residues` is a flatter view of the manually curated catalytic residues. The much larger
//! homologue-residue download is deliberately not fetched here because it exceeds 200 MB and the
//! M-CSA documentation warns that transferred residues may not be conserved or functional.

use serde::Deserialize;

use crate::{ReqError, make_agent};

const EBI_ORIGIN: &str = "https://www.ebi.ac.uk";
const API_URL: &str = "https://www.ebi.ac.uk/thornton-srv/m-csa/api";

const USER_AGENT: &str = concat!(
    "bio_apis/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/David-OConnor/bio_apis)"
);

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ProteinSequence {
    pub uniprot_id: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Protein {
    pub sequences: Vec<ProteinSequence>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ResidueRole {
    /// A broad role, e.g. "proton shuttle (general acid/base)".
    pub group_function: String,
    /// "reactant", "interaction", or "spectator".
    pub function_type: String,
    /// A specific role, e.g. "proton acceptor".
    pub function: String,
    /// Enzyme Mechanism Ontology identifier, e.g. "EMO_00066".
    pub emo: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ResidueChain {
    pub chain_name: String,
    pub pdb_id: String,
    pub assembly_chain_name: String,
    pub assembly: Option<u32>,
    /// Three-letter amino-acid code, e.g. "Asp".
    pub code: String,
    /// Residue number in M-CSA's biological assembly.
    pub resid: Option<i32>,
    /// Author residue number from the PDB structure.
    pub auth_resid: Option<i32>,
    pub is_reference: bool,
    pub domain_name: String,
    pub domain_cath_id: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct ResidueSequence {
    pub uniprot_id: String,
    /// Three-letter amino-acid code, e.g. "Asp".
    pub code: String,
    pub is_reference: bool,
    /// 1-based residue number in the UniProt sequence.
    pub resid: Option<i32>,
}

/// One manually curated catalytic residue and its mappings into sequence and structure.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct CatalyticResidue {
    pub mcsa_id: u32,
    pub roles_summary: String,
    pub function_location_abv: String,
    pub main_annotation: String,
    pub ptm: String,
    pub roles: Vec<ResidueRole>,
    pub residue_chains: Vec<ResidueChain>,
    pub residue_sequences: Vec<ResidueSequence>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Compound {
    pub count: u32,
    /// "reactant" or "product".
    #[serde(rename = "type")]
    pub type_: String,
    /// Numeric ChEBI identifier, represented as text by M-CSA.
    pub chebi_id: String,
    pub name: String,
    pub mol_file: String,
}

impl Compound {
    pub fn chebi_id(&self) -> Option<u32> {
        self.chebi_id.trim_start_matches("CHEBI:").parse().ok()
    }

    /// Turn M-CSA's scheme and mol-file paths, which may omit the scheme, into HTTPS URLs.
    pub fn mol_url(&self) -> String {
        absolute_url(&self.mol_file)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct MechanismStep {
    pub step_id: u32,
    pub description: String,
    pub figure: String,
    pub is_product: bool,
    pub marvin_xml: String,
}

impl MechanismStep {
    pub fn figure_url(&self) -> String {
        absolute_url(&self.figure)
    }

    pub fn marvin_url(&self) -> String {
        absolute_url(&self.marvin_xml)
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Reference {
    pub pubmed_id: String,
    pub doi: String,
    pub title: String,
    pub evidence_types: Vec<String>,
}

impl Reference {
    pub fn pubmed_id(&self) -> Option<u32> {
        self.pubmed_id.parse().ok()
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Mechanism {
    pub mechanism_id: u32,
    pub is_detailed: bool,
    pub mechanism_text: String,
    pub rating: u8,
    pub components_summary: String,
    pub steps: Vec<MechanismStep>,
    pub references: Vec<Reference>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Reaction {
    pub ec: String,
    pub compounds: Vec<Compound>,
    pub mechanisms: Vec<Mechanism>,
    pub is_polymeric: bool,
}

impl Reaction {
    pub fn reactants(&self) -> impl Iterator<Item = &Compound> {
        self.compounds.iter().filter(|c| c.type_ == "reactant")
    }

    pub fn products(&self) -> impl Iterator<Item = &Compound> {
        self.compounds.iter().filter(|c| c.type_ == "product")
    }
}

/// A curated M-CSA enzyme-mechanism entry.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct Entry {
    pub mcsa_id: u32,
    pub enzyme_name: String,
    pub is_reference_uniprot_id: bool,
    pub reference_uniprot_id: String,
    pub url: String,
    pub description: String,
    pub protein: Protein,
    pub all_ecs: Vec<String>,
    pub residues: Vec<CatalyticResidue>,
    pub reaction: Reaction,
}

impl Entry {
    pub fn url(&self) -> String {
        absolute_url(&self.url)
    }

    /// Load the reference sequence's UniProt entry, preserving the direct M-CSA/UniProt bridge.
    pub fn load_uniprot(
        &self,
        fields: &[crate::uniprot::Field],
    ) -> Result<crate::uniprot::Protein, ReqError> {
        crate::uniprot::load_protein_fields(&self.reference_uniprot_id, fields)
    }

    /// Find Rhea reactions for every EC number attached to this mechanistic entry.
    pub fn rhea_reactions(
        &self,
        limit_per_ec: Option<u32>,
    ) -> Result<Vec<crate::rhea::Reaction>, ReqError> {
        let mut result = Vec::new();
        for ec in &self.all_ecs {
            for reaction in crate::rhea::reactions_from_ec(ec, limit_per_ec)? {
                if !result
                    .iter()
                    .any(|r: &crate::rhea::Reaction| r.id == reaction.id)
                {
                    result.push(reaction);
                }
            }
        }
        Ok(result)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct EntryPage {
    next: Option<String>,
    results: Vec<Entry>,
}

fn absolute_url(path: &str) -> String {
    if path.starts_with("https://") || path.starts_with("http://") {
        path.to_owned()
    } else if path.starts_with('/') {
        format!("{EBI_ORIGIN}{path}")
    } else {
        format!("https://{path}")
    }
}

fn get(url: &str) -> Result<String, ReqError> {
    let mut resp = make_agent()
        .get(url)
        .header("User-Agent", USER_AGENT)
        .call()?;
    if resp.status() != 200 {
        return Err(ReqError::Http);
    }
    Ok(resp.body_mut().read_to_string()?)
}

fn entry_query(parameters: &[(&str, String)], limit: Option<u32>) -> Result<Vec<Entry>, ReqError> {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("format", "json");
    for (key, value) in parameters {
        params.append_pair(key, value);
    }

    let mut next = Some(format!("{API_URL}/entries/?{}", params.finish()));
    let mut result = Vec::new();

    while let Some(url) = next {
        let page: EntryPage = serde_json::from_str(&get(&absolute_url(&url))?)?;
        if page.results.is_empty() {
            break;
        }
        result.extend(page.results);
        if let Some(limit) = limit
            && result.len() >= limit as usize
        {
            result.truncate(limit as usize);
            break;
        }
        next = page.next;
    }

    Ok(result)
}

fn residue_query(parameters: &[(&str, String)]) -> Result<Vec<CatalyticResidue>, ReqError> {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("format", "json");
    for (key, value) in parameters {
        params.append_pair(key, value);
    }
    let url = format!("{API_URL}/residues/?{}", params.finish());
    Ok(serde_json::from_str(&get(&url)?)?)
}

/// Load one M-CSA entry by its numeric identifier.
pub fn load_entry(mcsa_id: u32) -> Result<Entry, ReqError> {
    entry_query(&[("entries.mcsa_ids", mcsa_id.to_string())], Some(1))?
        .into_iter()
        .next()
        .ok_or(ReqError::Deserialize)
}

/// Browse mechanism entries. Pass a limit unless the complete atlas is intended.
pub fn entries(limit: Option<u32>) -> Result<Vec<Entry>, ReqError> {
    entry_query(&[], limit)
}

/// Load several entries in one filtered request.
pub fn entries_from_ids(mcsa_ids: &[u32]) -> Result<Vec<Entry>, ReqError> {
    if mcsa_ids.is_empty() {
        return Ok(Vec::new());
    }
    let ids = mcsa_ids
        .iter()
        .map(u32::to_string)
        .collect::<Vec<_>>()
        .join(",");
    entry_query(&[("entries.mcsa_ids", ids)], Some(mcsa_ids.len() as u32))
}

/// Find mechanism entries for one or more EC numbers.
pub fn entries_from_ec(ec_numbers: &[String], limit: Option<u32>) -> Result<Vec<Entry>, ReqError> {
    if ec_numbers.is_empty() {
        return Ok(Vec::new());
    }
    entry_query(
        &[("entries.reactions.ecs.codes", ec_numbers.join(","))],
        limit,
    )
}

/// Find mechanism entries linked to a UniProtKB accession.
pub fn entries_from_uniprot(accession: &str, limit: Option<u32>) -> Result<Vec<Entry>, ReqError> {
    entry_query(
        &[(
            "entries.proteins.sequences.uniprot_ids",
            accession.trim_start_matches("UniProtKB:").to_owned(),
        )],
        limit,
    )
}

/// Load M-CSA's curated catalytic residues for one entry.
pub fn residues_from_entry(mcsa_id: u32) -> Result<Vec<CatalyticResidue>, ReqError> {
    residue_query(&[("entries.mcsa_ids", mcsa_id.to_string())])
}

/// Load curated catalytic residues for one or more EC numbers.
pub fn residues_from_ec(ec_numbers: &[String]) -> Result<Vec<CatalyticResidue>, ReqError> {
    if ec_numbers.is_empty() {
        return Ok(Vec::new());
    }
    residue_query(&[("entries.reactions.ecs.codes", ec_numbers.join(","))])
}

/// Load curated catalytic residues by the reference or homologous UniProtKB sequence.
pub fn residues_from_uniprot(accession: &str) -> Result<Vec<CatalyticResidue>, ReqError> {
    residue_query(&[(
        "entries.proteins.sequences.uniprot_ids",
        accession.trim_start_matches("UniProtKB:").to_owned(),
    )])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_api_asset_urls() {
        assert_eq!(
            absolute_url("www.ebi.ac.uk/thornton-srv/m-csa/entry/1/"),
            "https://www.ebi.ac.uk/thornton-srv/m-csa/entry/1/"
        );
        assert_eq!(
            absolute_url("/thornton-srv/m-csa/entry/1/"),
            "https://www.ebi.ac.uk/thornton-srv/m-csa/entry/1/"
        );
    }
}
