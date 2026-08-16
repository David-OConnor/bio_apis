//! [Home page](https://www.ebi.ac.uk/chebi/)
//! [Tools](https://www.ebi.ac.uk/chebi/tools)
//! [API docs](https://www.ebi.ac.uk/chebi/backend/api/docs/)
//!
//! ChEBI (Chemical Entities of Biological Interest) is a dictionary of small molecules, hosted
//! by the EBI.
//!
//! Identifiers here are ChEBI accessions. Functions accept either the prefixed form
//! (`CHEBI:46195`, case-insensitive) or the bare number (`46195`).
//!
//! Text fields from ChEBI (names, definitions) may contain HTML markup, e.g.
//! `<em>N</em>-acetyl-<em>p</em>-aminophenol`. The `ascii_name` fields are the markup-free
//! equivalents.

use std::collections::HashMap;

use serde::Deserialize;
use serde_aux::prelude::*;

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://www.ebi.ac.uk/chebi";

const API_URL: &str = "https://www.ebi.ac.uk/chebi/backend/api/public";

/// A structure (connection table) associated with a compound.
#[derive(Clone, Debug, Deserialize)]
pub struct Structure {
    pub id: u32,
    pub smiles: Option<String>,
    pub standard_inchi: Option<String>,
    pub standard_inchi_key: Option<String>,
    /// Web3 Unique Representation of Carbohydrate Structures; carbohydrates only.
    pub wurcs: Option<String>,
    /// If true, this is a generic (Markush) structure with R groups, e.g. for a compound class.
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub is_r_group: bool,
}

/// Note: ChEBI serves masses as strings; we parse them to floats.
#[derive(Clone, Debug, Deserialize)]
pub struct ChemicalData {
    pub formula: Option<String>,
    pub charge: Option<i16>,
    /// Average mass, in Da.
    #[serde(default, deserialize_with = "deserialize_option_number_from_string")]
    pub mass: Option<f32>,
    /// Monoisotopic mass, in Da.
    #[serde(default, deserialize_with = "deserialize_option_number_from_string")]
    pub monoisotopic_mass: Option<f32>,
}

/// One name (synonym, brand name, IUPAC name etc) of a compound.
#[derive(Clone, Debug, Deserialize)]
pub struct Name {
    pub name: String,
    /// The markup-free variant of `name`.
    pub ascii_name: Option<String>,
    /// E.g. "SYNONYM", "IUPAC NAME", "BRAND NAME", "INN".
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// E.g. "C" for curated, "S" for submitted.
    pub status: Option<String>,
    pub source: Option<String>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub adapted: bool,
    pub language_code: Option<String>,
}

/// A cross reference to another database, e.g. CAS, KEGG, DrugBank, PubMed.
#[derive(Clone, Debug, Deserialize)]
pub struct DatabaseAccession {
    pub id: u32,
    pub accession_number: String,
    /// E.g. "CAS", "CITATION", "MANUAL_X_REF", "REGISTRY_NUMBER".
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub source_name: Option<String>,
    pub url: Option<String>,
    /// The [bioregistry](https://bioregistry.io) prefix, e.g. "drugbank", "chemspider".
    pub prefix: Option<String>,
}

/// A single edge in the ChEBI ontology, e.g. "paracetamol" *has role* "antipyretic".
#[derive(Clone, Debug, Deserialize)]
pub struct OntologyRelation {
    pub init_id: u32,
    pub init_name: String,
    /// E.g. "is a", "has role", "has functional parent", "has part".
    pub relation_type: String,
    pub final_id: u32,
    pub final_name: String,
}

/// Outgoing relations point away from this compound (its parents); incoming ones point at it
/// (its children).
#[derive(Clone, Debug, Default, Deserialize)]
pub struct OntologyRelations {
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub incoming_relations: Vec<OntologyRelation>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub outgoing_relations: Vec<OntologyRelation>,
}

/// A role (biological, chemical, or application) this compound has, from the ontology.
#[derive(Clone, Debug, Deserialize)]
pub struct RoleClassification {
    pub id: u32,
    pub chebi_accession: String,
    pub name: String,
    pub definition: Option<String>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub biological_role: bool,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub chemical_role: bool,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub application: bool,
}

/// Where this compound has been observed, e.g. a species and tissue.
#[allow(unused)]
#[derive(Clone, Debug, Deserialize)]
pub struct CompoundOrigin {
    pub species_text: Option<String>,
    pub species_accession: Option<String>,
    pub component_text: Option<String>,
    pub component_accession: Option<String>,
    pub strain_text: Option<String>,
    pub strain_accession: Option<String>,
    pub source: Option<String>,
    pub source_accession: Option<String>,
    pub comments: Option<String>,
}

/// A full ChEBI entity record.
#[derive(Clone, Debug, Deserialize)]
pub struct Compound {
    /// The numeric portion of the accession, e.g. 46195.
    pub id: u32,
    /// E.g. "CHEBI:46195".
    pub chebi_accession: String,
    pub name: Option<String>,
    pub ascii_name: Option<String>,
    /// Curation quality, 1-3. Most of ChEBI's manually annotated entries are 3-star.
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub stars: u8,
    pub definition: Option<String>,
    /// Keyed by name type, e.g. "SYNONYM", "IUPAC NAME", "BRAND NAME".
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub names: HashMap<String, Vec<Name>>,
    /// Absent for entities that aren't molecules, e.g. roles.
    pub chemical_data: Option<ChemicalData>,
    /// Absent for entities with no structure, e.g. roles.
    pub default_structure: Option<Structure>,
    /// Deprecated accessions that now resolve to this entry.
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub secondary_ids: Vec<String>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub compound_origins: Vec<CompoundOrigin>,
    /// Keyed by accession type, e.g. "CAS", "MANUAL_X_REF", "REGISTRY_NUMBER", "CITATION".
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub database_accessions: HashMap<String, Vec<DatabaseAccession>>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub ontology_relations: OntologyRelations,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub roles_classification: Vec<RoleClassification>,
    /// ISO-8601, e.g. "2023-10-05T15:14:38Z".
    pub modified_on: Option<String>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub is_released: bool,
}

impl Compound {
    /// The first name of a given type, e.g. "IUPAC NAME" or "BRAND NAME". Prefers the
    /// markup-free variant.
    pub fn name_of_type(&self, type_: &str) -> Option<String> {
        let n = self.names.get(type_)?.first()?;
        Some(n.ascii_name.clone().unwrap_or_else(|| n.name.clone()))
    }

    /// Cross-reference accessions for a given source, e.g. "DrugBank", "KEGG COMPOUND",
    /// "ChemSpider". Sources are spread across accession types, so we search all of them.
    pub fn xrefs_from_source(&self, source: &str) -> Vec<String> {
        self.database_accessions
            .values()
            .flatten()
            .filter(|a| a.source_name.as_deref() == Some(source))
            .map(|a| a.accession_number.clone())
            .collect()
    }
}

/// Deserializing only; the batch compounds endpoint wraps each record with resolution metadata.
#[derive(Debug, Deserialize)]
struct CompoundEntry {
    data: Option<Compound>,
}

/// A curated subset of a ChEBI record, for applications that don't need the full entity.
/// Analogous to `pubchem::Properties`.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct Properties {
    /// The numeric portion of the accession, e.g. 46195.
    pub id: u32,
    /// The primary ChEBI name, markup-free where available.
    pub name: String,
    /// Chemical name systematically determined according to the IUPAC nomenclatures.
    pub iupac_name: Option<String>,
    pub definition: Option<String>,
    pub formula: Option<String>,
    /// Average mass, in Da.
    pub mass: Option<f32>,
    /// Monoisotopic mass, in Da.
    pub monoisotopic_mass: Option<f32>,
    /// Net charge at the pH ChEBI curates for (7.3).
    pub charge: Option<i16>,
    /// A SMILES (Simplified Molecular Input Line Entry System) string.
    pub smiles: Option<String>,
    /// Standard IUPAC International Chemical Identifier (InChI).
    pub inchi: Option<String>,
    /// Hashed version of the full standard InChI, consisting of 27 characters.
    pub inchi_key: Option<String>,
}

impl From<&Compound> for Properties {
    fn from(c: &Compound) -> Self {
        let (formula, mass, monoisotopic_mass, charge) = match &c.chemical_data {
            Some(d) => (d.formula.clone(), d.mass, d.monoisotopic_mass, d.charge),
            None => (None, None, None, None),
        };

        let (smiles, inchi, inchi_key) = match &c.default_structure {
            Some(s) => (
                s.smiles.clone(),
                s.standard_inchi.clone(),
                s.standard_inchi_key.clone(),
            ),
            None => (None, None, None),
        };

        Self {
            id: c.id,
            name: c
                .ascii_name
                .clone()
                .or_else(|| c.name.clone())
                .unwrap_or_default(),
            iupac_name: c.name_of_type("IUPAC NAME"),
            definition: c.definition.clone(),
            formula,
            mass,
            monoisotopic_mass,
            charge,
            smiles,
            inchi,
            inchi_key,
        }
    }
}

/// One hit from a text or structure search. ChEBI returns these in Elasticsearch's envelope.
#[derive(Clone, Debug, Deserialize)]
pub struct SearchHit {
    /// The numeric portion of the accession, e.g. 46195.
    #[serde(rename = "_id", deserialize_with = "deserialize_number_from_string")]
    pub id: u32,
    /// Relevance score. Structure searches return 1.0 for every hit.
    #[serde(rename = "_score")]
    pub score: Option<f32>,
    #[serde(rename = "_source")]
    pub data: SearchHitData,
}

/// The compound summary carried by a search hit. This is a subset of `Compound`; use
/// `load_compound` for the full record.
#[derive(Clone, Debug, Deserialize)]
pub struct SearchHitData {
    pub chebi_accession: String,
    pub name: Option<String>,
    pub ascii_name: Option<String>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub stars: u8,
    pub definition: Option<String>,
    pub formula: Option<String>,
    pub mass: Option<f32>,
    #[serde(rename = "monoisotopicmass")]
    pub monoisotopic_mass: Option<f32>,
    pub charge: Option<i16>,
    pub smiles: Option<String>,
    pub inchi: Option<String>,
    #[serde(rename = "inchikey")]
    pub inchi_key: Option<String>,
    /// Structure ids; the first is generally the default structure.
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub structures: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct SearchResults {
    pub results: Vec<SearchHit>,
    /// Total hits available, across all pages.
    pub total: u32,
    pub number_pages: u32,
}

/// https://www.ebi.ac.uk/chebi/backend/api/docs/#/public/public_structure_search_retrieve
#[derive(Clone, Copy, PartialEq)]
pub enum StructureSearchType {
    /// Same connectivity, ignoring stereochemistry and isotopes.
    Connectivity,
    Similarity,
    Substructure,
}

impl std::fmt::Display for StructureSearchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Connectivity => "connectivity",
            Self::Similarity => "similarity",
            Self::Substructure => "substructure",
        };
        write!(f, "{v}")
    }
}

/// Accepts `CHEBI:46195`, `chebi:46195`, or `46195`; returns the numeric portion.
/// Can be used if the input format uses a string variant.
pub fn parse_id(ident: &str) -> Result<u32, ReqError> {
    let v = ident.trim();

    let digits = match v.split_once(':') {
        Some((prefix, num)) if prefix.eq_ignore_ascii_case("chebi") => num,
        Some(_) => return Err(ReqError::Deserialize),
        None => v,
    };

    digits.trim().parse().map_err(|_| ReqError::Deserialize)
}

/// Our agent doesn't treat error status codes as errors, and ChEBI returns a JSON body on
/// failure, e.g. `{"detail": "No Compound matches the given query."}`. Catch that here, so we
/// don't hand a failure message back to the caller as if it were data.
fn get(url: &str) -> Result<String, ReqError> {
    let agent = make_agent();
    let mut resp = agent.get(url).call()?;

    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    Ok(resp.body_mut().read_to_string()?)
}

pub fn open_overview(id: u32) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/CHEBI:{id}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Load the full ChEBI record for an entity. Secondary (deprecated) accessions resolve to their
/// primary entry.
pub fn load_compound(id: u32) -> Result<Compound, ReqError> {
    let url = format!("{API_URL}/compound/{id}/");

    Ok(serde_json::from_str(&get(&url)?)?)
}

/// Load several records in a single request. Accessions ChEBI doesn't recognize are simply absent
/// from the result; the caller should chunk very large lists, as a long request URL can be
/// rejected.
pub fn load_compounds(idents: &[u32]) -> Result<Vec<Compound>, ReqError> {
    if idents.is_empty() {
        return Ok(Vec::new());
    }

    let ids: Vec<String> = idents
        .iter()
        .map(|i| i.to_string())
        .collect();

    let url = format!("{API_URL}/compounds/?chebi_ids={}", ids.join(","));

    let parsed: HashMap<String, CompoundEntry> = serde_json::from_str(&get(&url)?)?;

    Ok(parsed.into_values().filter_map(|e| e.data).collect())
}

/// A curated subset of a record's data. See `load_compound` for everything ChEBI has.
pub fn properties(ident: u32) -> Result<Properties, ReqError> {
    Ok((&load_compound(ident)?).into())
}

/// Get the Simplified Molecular Input Line Entry System (SMILES) representation of an entity.
pub fn get_smiles(ident: u32) -> Result<String, ReqError> {
    load_compound(ident)?
        .default_structure
        .and_then(|s| s.smiles)
        .ok_or(ReqError::Deserialize)
}

fn mol_url(id: u32) -> String {
    format!("{API_URL}/molfile/{id}/")
}

/// Download an MDL Molfile from ChEBI, returning a Molfile string. Note that these are 2D, and
/// that ChEBI only serves them for parent compounds.
///
/// Returns `ReqError::Deserialize` for entities with no structure; ChEBI answers those with an
/// empty body.
pub fn load_mol(id: u32) -> Result<String, ReqError> {
    let result = get(&mol_url(id))?;

    if result.trim().is_empty() {
        return Err(ReqError::Deserialize);
    }

    Ok(result)
}

/// Download a structure from ChEBI as an SDF string. ChEBI serves MDL Molfiles, which are SDF
/// records without the `$$$$` terminator (and without data fields), so we append one. Note that
/// these are 2D.
pub fn load_sdf(ident: u32) -> Result<String, ReqError> {
    let mol = load_mol(ident)?;

    Ok(format!("{}\n$$$$\n", mol.trim_end()))
}

/// Run a general text search. This accepts, among others: names, brand names, IUPAC names,
/// partial names, definitions, ChEBI ids, InChIs, InChI keys, SMILES, formulas, CAS numbers, and
/// cross-reference ids from other databases.
///
/// `page` is 1-based; ChEBI defaults to a size of 15 per page.
pub fn search(term: &str, page: u32, size: u32) -> Result<SearchResults, ReqError> {
    let term_sanitized = url::form_urlencoded::byte_serialize(term.as_bytes()).collect::<String>();
    let url = format!("{API_URL}/es_search/?term={term_sanitized}&page={page}&size={size}");

    Ok(serde_json::from_str(&get(&url)?)?)
}

/// Load a list of ChEBI ids from a text search. Analogous to `pubchem::find_cids_from_search`.
pub fn find_ids_from_search(term: &str) -> Result<Vec<u32>, ReqError> {
    Ok(search(term, 1, 15)?
        .results
        .into_iter()
        .map(|r| r.id)
        .collect())
}

/// Search by structure, using a SMILES query. `similarity` is the minimum similarity, from 0.4 to
/// 1.0; it applies to `StructureSearchType::Similarity` only.
///
/// If `three_star_only` is true, this only returns ChEBI's manually annotated 3-star entries.
pub fn structure_search(
    smiles: &str,
    search_type: StructureSearchType,
    similarity: Option<f32>,
    three_star_only: bool,
    page: u32,
    size: u32,
) -> Result<SearchResults, ReqError> {
    // SMILES contain characters like `#` and `+` that would otherwise be eaten by the query string.
    let smiles_sanitized =
        url::form_urlencoded::byte_serialize(smiles.as_bytes()).collect::<String>();

    let mut url = format!(
        "{API_URL}/structure_search/?smiles={smiles_sanitized}&search_type={search_type}\
        &three_star_only={three_star_only}&page={page}&size={size}"
    );

    if let Some(s) = similarity {
        url += &format!("&similarity={s}");
    }

    Ok(serde_json::from_str(&get(&url)?)?)
}

/// Find similar molecules from a SMILES representation. `similarity` is the minimum similarity,
/// from 0.4 to 1.0. Analogous to `pubchem::find_similar_mols`.
pub fn find_similar_mols(smiles: &str, similarity: f32) -> Result<Vec<u32>, ReqError> {
    Ok(structure_search(
        smiles,
        StructureSearchType::Similarity,
        Some(similarity),
        true,
        1,
        15,
    )?
    .results
    .into_iter()
    .map(|r| r.id)
    .collect())
}

/// Deserializing only; the ontology endpoints return relations under a compound stub.
#[derive(Debug, Deserialize)]
struct OntologyResp {
    ontology_relations: OntologyRelations,
}

/// Load the ontology parents of an entity, e.g. the classes it *is a*, and the roles it *has*.
pub fn load_parents(id: u32) -> Result<Vec<OntologyRelation>, ReqError> {
    let url = format!("{API_URL}/ontology/parents/CHEBI:{id}/");

    let parsed: OntologyResp = serde_json::from_str(&get(&url)?)?;
    Ok(parsed.ontology_relations.outgoing_relations)
}

/// Load the ontology children of an entity, e.g. the compounds that *are a* member of this class.
/// Note that this can be a large response for high-level classes.
pub fn load_children(id: u32) -> Result<Vec<OntologyRelation>, ReqError> {
    let url = format!("{API_URL}/ontology/children/CHEBI:{id}/");

    let parsed: OntologyResp = serde_json::from_str(&get(&url)?)?;
    Ok(parsed.ontology_relations.incoming_relations)
}
