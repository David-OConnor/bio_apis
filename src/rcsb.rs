//! For loading data from the RCSB website's API. This is a good option compared to
//! UniProt for downloading 3d structure.

//! PDB Search API: https://search.rcsb.org/#search-api
//! PDB Data API: https://data.rcsb.org/#data-api
//!
//! This module also covers small molecules (ligands), via the [Chemical Component Dictionary](https://www.wwpdb.org/data/ccd)
//! (CCD). See the section of this module marked as such.

use std::{
    io,
    io::{ErrorKind, Read},
};

#[cfg(feature = "encode")]
use bincode::{Decode, Encode};
use flate2::read::GzDecoder;
// todo: Determine if you want this.
use na_seq::{AminoAcid, seq_aa_to_str};
use rand::{self, RngExt};
use serde::{Deserialize, Serialize, Serializer};
use serde_aux::prelude::*;
use serde_json::{self};
use ureq::{
    self, Agent, Body,
    http::{Response, StatusCode},
};

use crate::{ReqError, chebi, make_agent, make_agent_with_timeout};

const BASE_URL: &str = "https://www.rcsb.org/structure";

const RCSB_3D_VIEW_URL: &str = "https://www.rcsb.org/3d-view";
const STRUCTURE_FILE_URL: &str = "https://files.rcsb.org/view";

const SEARCH_API_URL: &str = "https://search.rcsb.org/rcsbsearch/v2/query";
const DATA_API_URL: &str = "https://data.rcsb.org/rest/v1/core/entry";

// An arbitrary limit to prevent excessive queries to the PDB data api,
// and to simplify display code.
const MAX_RESULTS: usize = 8;

#[derive(Default, Serialize)]
pub struct PdbSearchParams {
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<String>,
    /// "protein". Not sure what other values are authorized.
    #[serde(skip_serializing_if = "Option::is_none")]
    sequence_type: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    evalue_cutoff: Option<u8>,
    #[serde(skip_serializing_if = "Option::is_none")]
    identity_cutoff: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    operator: Option<Operator>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ///https://search.rcsb.org/structure-search-attributes.html
    attribute: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pattern: Option<String>,
    /// Chemical service only: "descriptor" (SMILES or InChI), or "formula".
    #[serde(rename = "type", skip_serializing_if = "Option::is_none")]
    chem_query_type: Option<String>,
    /// Chemical service only.
    #[serde(skip_serializing_if = "Option::is_none")]
    descriptor_type: Option<DescriptorType>,
    /// Chemical service only.
    #[serde(skip_serializing_if = "Option::is_none")]
    match_type: Option<ChemMatchType>,
}

/// https://search.rcsb.org/#return-type
#[derive(Clone, Copy, Default)]
pub enum Operator {
    #[default]
    ExactMatch,
    Exists,
    Greater,
    Less,
    GreaterOrEqual,
    LessOrEqual,
    Equals,
    ContainsPhrase,
    ContainsWords,
    Range,
    In,
}

impl Serialize for Operator {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = match self {
            Self::ExactMatch => "exact_match",
            Self::Exists => "exists",
            Self::Greater => "greater",
            Self::Less => "less",
            Self::GreaterOrEqual => "greater_or_equal",
            Self::LessOrEqual => "less_or_equal",
            Self::Equals => "equals",
            Self::ContainsPhrase => "contains_phrase",
            Self::ContainsWords => "contains_words",
            Self::Range => "range",
            Self::In => "in",
        };

        serializer.serialize_str(str)
    }
}

/// https://search.rcsb.org/#return-type
#[derive(Clone, Copy, Default)]
pub enum ReturnType {
    #[default]
    Entry,
    Assembly,
    PolymerEntity,
    NonPolymerEntity,
    PolymerInstance,
    MolDefinition,
}

impl Serialize for ReturnType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = match self {
            Self::Entry => "entry",
            Self::Assembly => "assembly",
            Self::PolymerEntity => "polymer_entity",
            Self::NonPolymerEntity => "non_polymer_entity",
            Self::PolymerInstance => "polymer_instance",
            Self::MolDefinition => "mol_definition",
        };

        serializer.serialize_str(str)
    }
}

#[derive(Clone, Copy, Default)]
pub enum RcsbType {
    #[default]
    Terminal,
    Group,
}

impl Serialize for RcsbType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = match self {
            Self::Terminal => "terminal",
            Self::Group => "group",
        };

        serializer.serialize_str(str)
    }
}

#[derive(Clone, Copy, Default)]
pub enum Service {
    #[default]
    Text,
    FullText,
    TextChem,
    Structure,
    StrucMotif,
    Sequence,
    SeqMotif,
    Chemical,
}

impl Serialize for Service {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = match self {
            Self::Text => "text",
            Self::FullText => "full_text",
            Self::TextChem => "text_chem",
            Self::Structure => "structure",
            Self::StrucMotif => "strucmotif",
            Self::Sequence => "sequence",
            Self::SeqMotif => "seqmotif",
            Self::Chemical => "chemical",
        };

        serializer.serialize_str(str)
    }
}

/// The notation of a chemical search query.
/// https://search.rcsb.org/#chemical-search-service
#[derive(Clone, Copy, Default, PartialEq)]
pub enum DescriptorType {
    #[default]
    Smiles,
    InChI,
}

impl Serialize for DescriptorType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = match self {
            Self::Smiles => "SMILES",
            Self::InChI => "InChI",
        };

        serializer.serialize_str(str)
    }
}

/// How strictly a chemical search matches its query against CCD entries.
/// https://search.rcsb.org/#chemical-search-service
#[derive(Clone, Copy, Default, PartialEq)]
pub enum ChemMatchType {
    /// Same atoms, bonds, and stereochemistry; charges and bond orders must also match.
    GraphStrict,
    /// Same atoms, bonds, and stereochemistry, ignoring bond orders and charges.
    GraphRelaxedStereo,
    /// Same atoms and bonds, ignoring stereochemistry.
    #[default]
    GraphRelaxed,
    /// Similar, by chemical fingerprint (Tanimoto similarity).
    FingerprintSimilarity,
    /// The query is a substructure of the result. Fingerprint-screened; slower than the others.
    SubStructMatch,
}

impl Serialize for ChemMatchType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let str = match self {
            Self::GraphStrict => "graph-strict",
            Self::GraphRelaxedStereo => "graph-relaxed-stereo",
            Self::GraphRelaxed => "graph-relaxed",
            Self::FingerprintSimilarity => "fingerprint-similarity",
            Self::SubStructMatch => "sub-struct-graph-relaxed",
        };

        serializer.serialize_str(str)
    }
}

#[derive(Default, Serialize)]
pub struct PdbSearchQuery {
    /// "terminal", or "group"
    #[serde(rename = "type")]
    pub type_: RcsbType,
    pub service: Service,
    pub parameters: PdbSearchParams,
}

#[derive(Default, Serialize)]
pub struct Sort {
    pub sort_by: String,
    pub direction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub random_seed: Option<u32>,
}

#[derive(Default, Serialize)]
pub struct SearchRequestOptions {
    /// "sequence", "seqmotif", "structmotif", "structure", "chemical", or "text".
    /// Only for sequences?
    // todo: Enum
    #[serde(skip_serializing_if = "Option::is_none")]
    pub scoring_strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sort: Option<Vec<Sort>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub paginate: Option<Paginate>,
    /// Return every hit instead of a page of them. Only use this for queries known to be narrow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub return_all_hits: Option<bool>,
}

/// https://search.rcsb.org/#pagination
#[derive(Clone, Copy, Serialize)]
pub struct Paginate {
    /// 0-based.
    pub start: u32,
    pub rows: u32,
}

#[derive(Default, Serialize)]
pub struct PdbPayloadSearch {
    pub return_type: ReturnType,
    pub query: PdbSearchQuery,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_options: Option<SearchRequestOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_info: Option<String>,
}

#[derive(Default, Debug, Deserialize)]
pub struct PdbSearchResult {
    pub identifier: String,
    pub score: f32,
}

#[derive(Clone, Debug)]
pub struct PdbMetaData {
    // todo: A/R
    pub prim_cit_title: String,
}

#[derive(Default, Debug, Deserialize)]
pub struct PdbSearchResults {
    pub query_id: String,
    pub result_type: String,
    pub total_count: u32,
    pub result_set: Vec<PdbSearchResult>,
}

#[derive(Clone, Default, PartialEq, Debug, Deserialize)]
#[cfg_attr(feature = "encode", derive(Encode, Decode))]
pub struct PdbStruct {
    pub title: String,
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "encode", derive(Encode, Decode))]
pub struct Database2 {
    pub database_code: String,
    pub database_id: String,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "encode", derive(Encode, Decode))]
pub struct Cell {
    pub angle_alpha: f32,
    pub angle_beta: f32,
    pub angle_gamma: f32,
    pub length_a: f32,
    pub length_b: f32,
    pub length_c: f32,
    #[serde(rename = "Z_PDB", default)]
    pub zpdb: u8,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "encode", derive(Encode, Decode))]
pub struct Citation {
    pub country: Option<String>,
    pub id: String,
    pub journal_abbrev: String,
    #[serde(rename = "journal_id_ASTM")]
    pub journal_id_astm: Option<String>,
    #[serde(rename = "journal_id_CSD")]
    pub journal_id_csd: Option<String>,
    #[serde(rename = "journal_id_ISSN")]
    pub journal_id_issn: Option<String>,
    #[serde(default, deserialize_with = "deserialize_option_number_from_string")]
    pub journal_volume: Option<u16>,
    // #[serde(default, deserialize_with = "deserialize_option_number_from_string")]
    // pub page_first: Option<u32>,
    pub page_first: Option<String>, // todo: Sometimes int, sometimes string of int, sometimes non-int string.
    // #[serde(default, deserialize_with = "deserialize_option_number_from_string")]
    // pub page_last: Option<u32>,
    pub page_last: Option<String>,
    #[serde(rename = "pdbx_database_id_PubMed")]
    pub pdbx_database_id_pub_med: Option<u32>,
    pub rcsb_authors: Option<Vec<String>>,
    pub rcsb_is_primary: String,
    pub rcsb_journal_abbrev: String,
    pub title: Option<String>,
    pub year: Option<u16>,
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "encode", derive(Encode, Decode))]
pub struct PdbxDatabaseStatus {
    pub deposit_site: Option<String>,
    pub pdb_format_compatible: String,
    pub process_site: String,
    pub recvd_initial_deposition_date: String, // todo: Chrono time
    pub status_code: String,
    pub status_code_sf: Option<String>,
    #[serde(rename = "SG_entry")]
    pub sgentry: Option<String>,
}

#[derive(Clone, Default, Debug, PartialEq, Deserialize)]
#[cfg_attr(feature = "encode", derive(Encode, Decode))]
pub struct RcsbEntryInfo {
    pub assembly_count: u32,
    pub branched_entity_count: u32,
    pub cis_peptide_count: u32,
    pub deposited_atom_count: u32,
    pub deposited_deuterated_water_count: u32,
    pub deposited_hydrogen_atom_count: u32,
    pub deposited_model_count: u32,
    pub deposited_modeled_polymer_monomer_count: u32,
    pub deposited_nonpolymer_entity_instance_count: u32,
    pub deposited_polymer_entity_instance_count: u32,
    pub deposited_polymer_monomer_count: u32,
    pub deposited_solvent_atom_count: u32,
    pub deposited_unmodeled_polymer_monomer_count: u32,
    pub diffrn_radiation_wavelength_maximum: Option<f32>,
    pub diffrn_radiation_wavelength_minimum: Option<f32>,
    pub disulfide_bond_count: u32,
    pub entity_count: u32,
    pub experimental_method: String,
    pub experimental_method_count: u32,
    pub inter_mol_covalent_bond_count: u32,
    pub inter_mol_metalic_bond_count: u32,
    pub molecular_weight: f32,
    pub na_polymer_entity_types: String,
    pub nonpolymer_entity_count: u32,
    pub nonpolymer_molecular_weight_maximum: Option<f32>,
    pub nonpolymer_molecular_weight_minimum: Option<f32>,
    pub polymer_composition: String,
    pub polymer_entity_count: u32,
    #[serde(rename = "polymer_entity_count_DNA")]
    pub polymer_entity_count_dna: u32,
    #[serde(rename = "polymer_entity_count_RNA")]
    pub polymer_entity_count_rna: u32,
    pub polymer_entity_count_nucleic_acid: u32,
    pub polymer_entity_count_nucleic_acid_hybrid: u32,
    pub polymer_entity_count_protein: u32,
    pub polymer_entity_taxonomy_count: u32,
    pub polymer_molecular_weight_maximum: f32,
    pub polymer_molecular_weight_minimum: f32,
    pub polymer_monomer_count_maximum: u32,
    pub polymer_monomer_count_minimum: u32,
}

/// Top-level struct for results from the RCSB data API.
/// todo: Fill out fields A/R.
#[derive(Clone, Default, PartialEq, Debug, Deserialize)]
#[cfg_attr(feature = "encode", derive(Encode, Decode))]
pub struct PdbDataResults {
    #[serde(rename = "struct")]
    pub struct_: PdbStruct,
    #[serde(rename = "database_2", default)]
    pub database2: Vec<Database2>,
    pub cell: Option<Cell>,
    pub citation: Vec<Citation>,
    pub pdbx_database_status: PdbxDatabaseStatus,
    pub rcsb_entry_info: RcsbEntryInfo,
}

#[derive(Default, Debug, Deserialize)]
pub struct PrimaryCitation {
    pub title: String,
}

#[derive(Default, Debug, Deserialize)]
pub struct PdbMetaDataResults {
    pub rcsb_primary_citation: PrimaryCitation,
}

#[cfg_attr(feature = "encode", derive(Encode, Decode))]
pub struct PdbData {
    pub rcsb_id: String,
    pub title: String,
}

/// Get a semi-random protein released within the past week.
/// https://search.rcsb.org/#search-example-12
pub fn get_newly_released() -> Result<String, ReqError> {
    let payload_search = PdbPayloadSearch {
        return_type: ReturnType::Entry,
        query: PdbSearchQuery {
            type_: RcsbType::Terminal,
            service: Service::Text,
            parameters: PdbSearchParams {
                attribute: Some("rcsb_accession_info.initial_release_date".to_owned()),
                operator: Some(Operator::Greater),
                value: Some("now-1w".to_owned()),
                ..Default::default()
            },
        },
        ..Default::default()
    };

    let payload_json = serde_json::to_string(&payload_search).unwrap();

    let agent = make_agent();

    let resp: String = agent
        .post(SEARCH_API_URL)
        .header("Content-Type", "application/json")
        .send(&payload_json)?
        .body_mut()
        .read_to_string()?;

    let search_data: PdbSearchResults = serde_json::from_str(&resp)?;

    if search_data.result_set.is_empty() {
        Err(ReqError::Http)
    } else {
        let mut rng = rand::rng();
        let i = rng.random_range(0..search_data.result_set.len());

        Ok(search_data.result_set[i].identifier.clone())
    }
}

/// Load PDB data using [its API](https://search.rcsRb.org/#search-api)
/// Returns the set of PDB ID matches, with scores.
pub fn pdb_data_from_seq(aa_seq: &[AminoAcid]) -> Result<Vec<PdbData>, ReqError> {
    let payload_search = PdbPayloadSearch {
        return_type: ReturnType::Entry,
        query: PdbSearchQuery {
            type_: RcsbType::Terminal,
            service: Service::Sequence,
            parameters: PdbSearchParams {
                value: Some(seq_aa_to_str(aa_seq)),
                sequence_type: Some("protein".to_owned()),
                evalue_cutoff: Some(1),
                identity_cutoff: Some(0.9),
                ..Default::default()
            },
        },
        request_options: Some(SearchRequestOptions {
            scoring_strategy: Some("sequence".to_owned()),
            ..Default::default()
        }),
        ..Default::default()
    };

    // todo: Limit the query to our result cap, instead of indexing after?

    let payload_json = serde_json::to_string(&payload_search).unwrap();

    let agent = make_agent();

    let resp: String = agent
        .post(SEARCH_API_URL)
        .header("Content-Type", "application/json")
        .send(&payload_json)?
        .body_mut()
        .read_to_string()?;

    let search_data: PdbSearchResults = serde_json::from_str(&resp)?;

    let mut result_search = Vec::new();
    for (i, r) in search_data.result_set.into_iter().enumerate() {
        if i < MAX_RESULTS {
            result_search.push(r);
        }
    }

    let mut result = Vec::with_capacity(result_search.len());
    for r in result_search {
        let resp = agent
            .get(&format!("{DATA_API_URL}/{}", r.identifier))
            .call()?
            .body_mut()
            .read_to_string()?;

        let data: PdbDataResults = serde_json::from_str(&resp)?;

        result.push(PdbData {
            rcsb_id: r.identifier,
            title: data.struct_.title,
        })
    }

    Ok(result)
}

/// Open a PDB search for this protein's sequence, given a PDB ID, which we load from the API.
/// This works with 4-letter (legacy), and 12-letter IDs.
pub fn open_overview(ident: &str) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/{ident}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Open a PDB search for this protein's sequence, given a PDB ID, which we load from the API.
/// This works with 4-letter (legacy), and 12-letter IDs.
pub fn open_3d_view(ident: &str) {
    if let Err(e) = webbrowser::open(&format!("{RCSB_3D_VIEW_URL}/{ident}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Load PDB structure data in the PDBx/mmCIF format. This is a modern, text-based format.
/// It avoids the XML, and limitations of the other two available formats.
/// /// This works with 4-letter (legacy), and 12-letter IDs.
pub fn open_structure(ident: &str) {
    let url = format!("{STRUCTURE_FILE_URL}/{ident}.cif");

    if let Err(e) = webbrowser::open(&url) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

pub fn load_metadata(ident: &str) -> Result<PdbMetaData, ReqError> {
    let agent = make_agent();

    let resp = agent
        .get(&format!("{DATA_API_URL}/{}", data_api_ident(ident)))
        .call()?
        .body_mut()
        .read_to_string()?;

    let data: PdbMetaDataResults = serde_json::from_str(&resp)?;

    Ok(PdbMetaData {
        prim_cit_title: data.rcsb_primary_citation.title,
    })
}

/// An identifier in the form the Data API accepts. `data.rcsb.org` is keyed on the 4-character
/// entry ID and 404s on the 12-character extended one, e.g. `pdb_00004qz8` — unlike
/// `files.rcsb.org`, which takes either. An extended ID for an entry that has a 4-character
/// equivalent is that ID zero-padded, so the last 4 characters recover it. Anything else, including
/// a future entry with no 4-character form, is passed through unchanged.
fn data_api_ident(ident: &str) -> &str {
    if ident.len() == 12
        && ident
            .get(..8)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("pdb_0000"))
    {
        return &ident[8..];
    }

    ident
}

fn cif_url(ident: &str) -> String {
    format!(
        "https://files.rcsb.org/download/{}.cif",
        ident.to_uppercase()
    )
}

fn cif_gz_url(ident: &str) -> String {
    cif_url(ident) + ".gz"
}

/// Do not use directly: Helper for the 3 validation types.
/// This and the URL functions that call it are fallible due to needing part of the ident as part of the URL.
fn validation_base_url(ident: &str) -> io::Result<String> {
    if ident.len() < 3 {
        return Err(io::Error::new(
            ErrorKind::InvalidInput,
            "PDB ID must be >= 3 characters.",
        ));
    }

    Ok(format!(
        "https://files.rcsb.org/validation/download/{ident}_validation"
    ))
}

fn validation_cif_gz_url(ident: &str) -> io::Result<String> {
    Ok(validation_base_url(ident)? + ".cif.gz")
}

fn validation_2fo_fc_cif_gz_url(ident: &str) -> io::Result<String> {
    Ok(validation_base_url(ident)? + "_2fo-fc_map_coef.cif.gz")
}

fn validation_fo_fc_cif_gz_url(ident: &str) -> io::Result<String> {
    Ok(validation_base_url(ident)? + "_fo-fc_map_coef.cif.gz")
}

/// Load all data for a given RCSB PDB identifier.
/// todo: Missing most fields currently.
pub fn get_all_data(ident: &str) -> Result<PdbDataResults, ReqError> {
    let agent = make_agent();

    let resp = agent
        .get(&format!("{DATA_API_URL}/{}", data_api_ident(ident)))
        .call()?
        .body_mut()
        .read_to_string()?;

    Ok(serde_json::from_str(&resp)?)
}

pub fn map_gz_url(ident: &str) -> Result<String, ReqError> {
    // todo: Cut down on the required fields for this, to save data(?)
    let agent = make_agent();

    let resp = agent
        .get(&format!("{DATA_API_URL}/{}", data_api_ident(ident)))
        .call()?
        .body_mut()
        .read_to_string()?;

    // note: This DB ident is available under pdbx_database_related, rcsb_entry_container_identifiers, and rcsb_external_references

    let data: PdbDataResults = serde_json::from_str(&resp)?;

    for db in &data.database2 {
        if &db.database_id == "EMDB" {
            let ident_emdb = &db.database_code;
            let ident_emdb_2 = db.database_code.replace("-", "_").to_lowercase();

            return Ok(format!(
                // todo: We may need to use the data API for this. Example URL:
                // https://files.rcsb.org/pub/emdb/structures/EMD-39757/map/emd_39757.map.gz
                // todo: Can use the Data API to find this.
                "https://files.rcsb.org/pub/emdb/structures/{ident_emdb}/map/{ident_emdb_2}.map.gz",
            ));
        }
    }

    Err(ReqError::Http)
}

fn structure_factors_cif_url(ident: &str) -> String {
    format!(
        "https://files.rcsb.org/download/{}-sf.cif",
        ident.to_uppercase()
    )
}

fn structure_factors_cif_gz_url(ident: &str) -> String {
    structure_factors_cif_url(ident) + ".gz"
}

fn decode_gz_str_resp(resp: Response<Body>) -> Result<String, ReqError> {
    let body_reader = resp.into_body().into_reader();
    let mut decoder = GzDecoder::new(body_reader);

    let mut result = String::new();
    decoder.read_to_string(&mut result)?;

    Ok(result)
}

/// Download a (atomic coordinates) mmCIF file (protein atom coords and metadata) from the RCSB,
/// returning a CIF string. Downloads the compressed (.gz) version, then deocompresses, to save
/// bandwidth.
pub fn load_cif(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    let resp = agent.get(&cif_gz_url(ident)).call()?;
    if resp.status() != StatusCode::OK {
        return Err(ReqError::Http);
    }

    decode_gz_str_resp(resp)
}

/// Download a validation mmCIF file (Related to electron density??) from the RCSB, returning an CIF string.
///
pub fn load_validation_cif(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    let resp = agent
        .get(&validation_cif_gz_url(ident).unwrap_or_default())
        .call()?;
    decode_gz_str_resp(resp)
}

/// Download a validation 2fo_fc map mmCIF file (Related to reflections?) from the RCSB, returning an CIF string.
pub fn load_validation_2fo_fc_cif(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    let resp = agent
        .get(&validation_2fo_fc_cif_gz_url(ident).unwrap_or_default())
        .call()?;
    decode_gz_str_resp(resp)
}

/// Download a validation fo_fc map mmCIF file (related to reflections?) from the RCSB, returning an CIF string.
pub fn load_validation_fo_fc_cif(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    let resp = agent
        .get(&validation_fo_fc_cif_gz_url(ident).unwrap_or_default())
        .call()?;
    decode_gz_str_resp(resp)
}

/// Download a structure factors (e.g. computed electron density over space) mmCIF file
/// from the RCSB, returning an CIF string.
pub fn load_structure_factors_cif(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    let resp = agent.get(&structure_factors_cif_gz_url(ident)).call()?;
    decode_gz_str_resp(resp)
}

/// Download a map file (electron density, with DFT already applied), if available. (Usually not).
pub fn load_map(ident: &str) -> Result<Vec<u8>, ReqError> {
    let agent = make_agent();

    let resp = agent.get(&map_gz_url(ident)?).call()?;

    let body_reader = resp.into_body().into_reader();
    let mut decoder = GzDecoder::new(body_reader);

    let mut result = Vec::new();
    decoder.read_to_end(result.as_mut())?;

    Ok(result)
}

#[cfg_attr(feature = "encode", derive(Encode, Decode))]
#[derive(Clone, Debug, PartialEq)]
pub struct FilesAvailable {
    pub validation: bool,
    pub validation_2fo_fc: bool,
    pub validation_fo_fc: bool,
    pub structure_factors: bool,
    pub map: bool,
}

fn file_exists(url: &str, agent: &Agent) -> Result<bool, ReqError> {
    Ok(agent.head(url).call()?.status() == StatusCode::OK)
}

/// Find out if additional data files are available, such as structure factors and validation data.
pub fn get_files_avail(ident: &str) -> Result<FilesAvailable, ReqError> {
    let agent = make_agent();

    // With this check here, the validation URL checks will pass, so we can unwrap them.
    if ident.len() < 3 {
        return Err(ReqError::Io(io::Error::new(
            ErrorKind::InvalidData,
            "RCSB Ident too short",
        )));
    }

    let map = match &map_gz_url(ident) {
        Ok(url) => file_exists(url, &agent)?,
        Err(_) => false,
    };

    Ok(FilesAvailable {
        validation: file_exists(&validation_cif_gz_url(ident).unwrap(), &agent)?,
        validation_2fo_fc: file_exists(&validation_2fo_fc_cif_gz_url(ident).unwrap(), &agent)?,
        validation_fo_fc: file_exists(&validation_fo_fc_cif_gz_url(ident).unwrap(), &agent)?,
        structure_factors: file_exists(&structure_factors_cif_url(ident), &agent)?,
        map,
    })
}

// ---- Chemical Component Dictionary (CCD): ligands and other small molecules ---------------------
//
// The CCD is the wwPDB's dictionary of every chemical component that appears in a PDB entry:
// ligands, ions, solvents, and the monomers (amino acids, nucleotides, sugars) that make up
// polymers. Each component has a short ID of 1-5 uppercase alphanumeric characters, e.g. `MF8`
// (metformin), `ATP`, `HEM`, `ZN`, or `ALA`. These are the codes that appear as residue names in
// PDB and mmCIF files, and that tools like RFdiffusion3 use to refer to ligands. RCSB's pages call
// them ligands; its APIs call them `chem_comp` (chemical component), which we match in type names.
//
// Unlike PubChem and ChEBI entries, CCD entries are defined by how a molecule appears in a
// structure, so e.g. a compound bound in two protonation states may have two CCD entries.
//
// [Ligand page example](https://www.rcsb.org/ligand/MF8)
// [Data API schema](https://data.rcsb.org/redoc/index.html#tag/Chemical-Component-Service)
// [Search API chemical attributes](https://search.rcsb.org/chemical-search-attributes.html)

const LIGAND_PAGE_URL: &str = "https://www.rcsb.org/ligand";
const CHEM_COMP_API_URL: &str = "https://data.rcsb.org/rest/v1/core/chemcomp";
const LIGAND_FILE_URL: &str = "https://files.rcsb.org/ligands/download";

/// Results per page for CCD text and structure searches.
const CCD_SEARCH_ROWS: u32 = 25;

// In seconds. Text searches are fast; structure searches, especially substructure ones, can take
// several seconds, which is longer than our default timeout.
const SEARCH_TIMEOUT: u64 = 5;
const CHEM_SEARCH_TIMEOUT: u64 = 30;

/// Core attributes of a CCD entry. (The `chem_comp` block)
#[derive(Clone, Debug, Deserialize)]
pub struct ChemCompCore {
    /// The CCD ID, e.g. "MF8".
    pub id: String,
    /// The preferred name, e.g. "Metformin". This is often all-caps for older entries.
    pub name: Option<String>,
    /// E.g. "non-polymer", "L-peptide linking", "RNA linking", "D-saccharide, alpha linking".
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// Space-separated, by element, e.g. "C4 H11 N5".
    pub formula: Option<String>,
    /// In Da.
    pub formula_weight: Option<f32>,
    pub pdbx_formal_charge: Option<i16>,
    /// For modified residues, the standard residue this derives from, e.g. `["TRP"]`.
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub mon_nstd_parent_comp_id: Vec<String>,
    /// For amino acids and nucleotides.
    pub one_letter_code: Option<String>,
    pub three_letter_code: Option<String>,
    /// "Y" if this is an ambiguous placeholder, e.g. `UNL` (unknown ligand).
    pub pdbx_ambiguous_flag: Option<String>,
    /// E.g. "REL" (released), "OBS" (obsolete).
    pub pdbx_release_status: Option<String>,
    /// A CCD ID this entry supersedes.
    pub pdbx_replaces: Option<String>,
    /// ISO-8601
    pub pdbx_initial_date: Option<String>,
    /// ISO-8601
    pub pdbx_modified_date: Option<String>,
    pub pdbx_processing_site: Option<String>,
}

/// A line notation computed by a given program. (The `pdbx_chem_comp_descriptor` block)
#[derive(Clone, Debug, Deserialize)]
pub struct ChemCompDescriptor {
    pub descriptor: String,
    /// E.g. "SMILES", "SMILES_CANONICAL", "InChI", "InChIKey".
    #[serde(rename = "type")]
    pub type_: String,
    /// E.g. "CACTVS", "OpenEye OEToolkits", "ACDLabs", "InChI".
    pub program: Option<String>,
    pub program_version: Option<String>,
}

/// RCSB's single, preferred set of descriptors. (The `rcsb_chem_comp_descriptor` block)
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ChemCompDescriptors {
    /// Without stereochemistry.
    #[serde(rename = "SMILES")]
    pub smiles: Option<String>,
    /// With stereochemistry.
    #[serde(rename = "SMILES_stereo")]
    pub smiles_stereo: Option<String>,
    #[serde(rename = "InChI")]
    pub inchi: Option<String>,
    #[serde(rename = "InChIKey")]
    pub inchi_key: Option<String>,
}

/// A name computed by a given program. (The `pdbx_chem_comp_identifier` block)
#[derive(Clone, Debug, Deserialize)]
pub struct ChemCompIdentifier {
    pub identifier: String,
    /// E.g. "SYSTEMATIC NAME"
    #[serde(rename = "type")]
    pub type_: String,
    pub program: Option<String>,
    pub program_version: Option<String>,
}

/// Atom and bond counts, and release dates. (The `rcsb_chem_comp_info` block)
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ChemCompInfo {
    /// Includes hydrogens.
    pub atom_count: Option<u32>,
    pub atom_count_chiral: Option<u32>,
    pub atom_count_heavy: Option<u32>,
    pub bond_count: Option<u32>,
    pub bond_count_aromatic: Option<u32>,
    /// ISO-8601
    pub initial_deposition_date: Option<String>,
    /// ISO-8601
    pub initial_release_date: Option<String>,
    /// ISO-8601
    pub revision_date: Option<String>,
    pub release_status: Option<String>,
}

/// A cross-reference to another database. (The `rcsb_chem_comp_related` block)
#[derive(Clone, Debug, Deserialize)]
pub struct ChemCompRelated {
    /// E.g. "PubChem", "ChEBI", "DrugBank", "ChEMBL", "CAS", "CCDC/CSD", "Pharos".
    pub resource_name: String,
    /// E.g. "5957" for PubChem, "CHEBI:15422" for ChEBI, "DB00171" for DrugBank.
    pub resource_accession_code: String,
    /// E.g. "matching InChIKey in PubChem", "assigned by PubChem resource", "assigned by PDB".
    pub related_mapping_method: Option<String>,
    pub ordinal: Option<u32>,
}

/// (The `rcsb_chem_comp_container_identifiers` block)
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ChemCompContainerIdentifiers {
    pub drugbank_id: Option<String>,
    /// For entries that are part of a BIRD (Biologically Interesting Reference Dictionary) entry.
    pub prd_id: Option<String>,
    /// For multi-component entries, the CCD IDs of the parts.
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub subcomponent_ids: Vec<String>,
    /// WHO Anatomical Therapeutic Chemical classification codes; drugs only.
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub atc_codes: Vec<String>,
}

/// A name or synonym, with provenance. (The `rcsb_chem_comp_synonyms` block)
#[derive(Clone, Debug, Deserialize)]
pub struct ChemCompSynonym {
    pub name: String,
    /// E.g. "Preferred Name", "Synonym", "Systematic Name".
    #[serde(rename = "type")]
    pub type_: Option<String>,
    /// E.g. "PDB Reference Data", "DrugBank", "OpenEye OEToolkits".
    pub provenance_source: Option<String>,
    pub ordinal: Option<u32>,
}

/// A protein this compound acts on, e.g. a drug target. (The `rcsb_chem_comp_target` block)
#[derive(Clone, Debug, Deserialize)]
pub struct ChemCompTarget {
    /// The target protein's name.
    pub name: Option<String>,
    /// E.g. "target", "enzyme", "transporter", "carrier".
    pub interaction_type: Option<String>,
    /// E.g. "inhibitor", "activator", "substrate".
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub target_actions: Vec<String>,
    /// E.g. "UniProt".
    pub reference_database_name: Option<String>,
    /// E.g. a UniProt accession.
    pub reference_database_accession_code: Option<String>,
    /// E.g. "DrugBank".
    pub provenance_source: Option<String>,
    pub ordinal: Option<u32>,
}

/// One level of an annotation's hierarchy, e.g. an ATC class.
#[derive(Clone, Debug, Deserialize)]
pub struct AnnotationLineage {
    pub id: String,
    pub name: Option<String>,
    /// Absent for the root.
    pub depth: Option<u32>,
}

/// A classification, e.g. an ATC code. (The `rcsb_chem_comp_annotation` block)
#[derive(Clone, Debug, Deserialize)]
pub struct ChemCompAnnotation {
    pub annotation_id: Option<String>,
    pub name: Option<String>,
    /// E.g. "ATC"
    #[serde(rename = "type")]
    pub type_: Option<String>,
    pub description: Option<String>,
    pub provenance_source: Option<String>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub annotation_lineage: Vec<AnnotationLineage>,
}

/// A full CCD entry from the RCSB Data API. Blocks describing BIRD (peptide-like antibiotic and
/// inhibitor) reference molecules, `pdbx_reference_*`, are omitted.
#[derive(Clone, Debug, Deserialize)]
pub struct ChemComp {
    /// The CCD ID, e.g. "MF8".
    pub rcsb_id: String,
    pub chem_comp: ChemCompCore,
    /// Absent for placeholder entries like `UNL` (unknown ligand).
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub rcsb_chem_comp_descriptor: ChemCompDescriptors,
    /// All descriptors, from each program RCSB ran. `rcsb_chem_comp_descriptor` has the preferred
    /// ones.
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub pdbx_chem_comp_descriptor: Vec<ChemCompDescriptor>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub pdbx_chem_comp_identifier: Vec<ChemCompIdentifier>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub rcsb_chem_comp_info: ChemCompInfo,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub rcsb_chem_comp_container_identifiers: ChemCompContainerIdentifiers,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub rcsb_chem_comp_related: Vec<ChemCompRelated>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub rcsb_chem_comp_synonyms: Vec<ChemCompSynonym>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub rcsb_chem_comp_target: Vec<ChemCompTarget>,
    #[serde(default, deserialize_with = "deserialize_default_from_null")]
    pub rcsb_chem_comp_annotation: Vec<ChemCompAnnotation>,
}

impl ChemComp {
    /// Cross-reference accessions for a given source, in RCSB's order, e.g. "PubChem", "ChEBI",
    /// "DrugBank", "ChEMBL", "CAS", "CCDC/CSD".
    pub fn xrefs_from_source(&self, source: &str) -> Vec<String> {
        self.rcsb_chem_comp_related
            .iter()
            .filter(|r| r.resource_name == source)
            .map(|r| r.resource_accession_code.clone())
            .collect()
    }

    /// All PubChem CIDs RCSB maps this entry to, sorted ascending. RCSB maps by matching InChIKey,
    /// and PubChem sometimes has several compounds with the same one; the lowest CID is generally
    /// the canonical one, e.g. 5950 for `ALA` (L-alanine), and 5892 for `NAD`.
    pub fn pubchem_cids(&self) -> Vec<u32> {
        let mut result: Vec<u32> = self
            .xrefs_from_source("PubChem")
            .iter()
            .filter_map(|c| c.parse().ok())
            .collect();

        result.sort_unstable();
        result.dedup();
        result
    }

    /// All ChEBI ids RCSB maps this entry to (the numeric portion), sorted ascending. RCSB takes
    /// these from PubChem's record, which can list several related entities that share its
    /// InChIKey: e.g. a zwitterion, or, for `GLC` (alpha-D-glucose), five glucose polymers. Use
    /// `chebi_id_from_ccd` to pick the one that best matches this entry.
    pub fn chebi_ids(&self) -> Vec<u32> {
        let mut result: Vec<u32> = self
            .xrefs_from_source("ChEBI")
            .iter()
            .filter_map(|c| chebi::parse_id(c).ok())
            .collect();

        result.sort_unstable();
        result.dedup();
        result
    }

    /// The first systematic (IUPAC-style) name available.
    pub fn systematic_name(&self) -> Option<String> {
        self.pdbx_chem_comp_identifier
            .iter()
            .find(|i| i.type_.eq_ignore_ascii_case("SYSTEMATIC NAME"))
            .map(|i| i.identifier.clone())
    }
}

/// A curated subset of a CCD entry, for applications that don't need the full record.
/// Analogous to `pubchem::Properties` and `chebi::Properties`.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "encode", derive(Encode, Decode))]
pub struct CcdProperties {
    /// The CCD ID, e.g. "MF8".
    pub id: String,
    pub name: String,
    /// E.g. "non-polymer", "L-peptide linking".
    pub type_: Option<String>,
    /// Chemical name systematically determined according to the IUPAC nomenclatures.
    pub systematic_name: Option<String>,
    /// Space-separated, by element, e.g. "C4 H11 N5".
    pub formula: Option<String>,
    /// In Da.
    pub formula_weight: Option<f32>,
    pub formal_charge: Option<i16>,
    pub atom_count_heavy: Option<u32>,
    /// A SMILES (Simplified Molecular Input Line Entry System) string, including stereochemistry.
    pub smiles: Option<String>,
    /// Standard IUPAC International Chemical Identifier (InChI).
    pub inchi: Option<String>,
    /// Hashed version of the full standard InChI, consisting of 27 characters.
    pub inchi_key: Option<String>,
    /// The lowest mapped PubChem CID. See `ChemComp::pubchem_cids`.
    pub pubchem_cid: Option<u32>,
    /// The mapped ChEBI id that best matches this entry. See `chebi_id_from_ccd`.
    pub chebi_id: Option<u32>,
    pub drugbank_id: Option<String>,
}

impl From<&ChemComp> for CcdProperties {
    fn from(c: &ChemComp) -> Self {
        let d = &c.rcsb_chem_comp_descriptor;

        Self {
            id: c.rcsb_id.clone(),
            name: c.chem_comp.name.clone().unwrap_or_default(),
            type_: c.chem_comp.type_.clone(),
            systematic_name: c.systematic_name(),
            formula: c.chem_comp.formula.clone(),
            formula_weight: c.chem_comp.formula_weight,
            formal_charge: c.chem_comp.pdbx_formal_charge,
            atom_count_heavy: c.rcsb_chem_comp_info.atom_count_heavy,
            smiles: d.smiles_stereo.clone().or_else(|| d.smiles.clone()),
            inchi: d.inchi.clone(),
            inchi_key: d.inchi_key.clone(),
            pubchem_cid: c.pubchem_cids().first().copied(),
            // This may be refined by `ccd_properties`, which can make a ChEBI request.
            chebi_id: c.chebi_ids().first().copied(),
            drugbank_id: c.rcsb_chem_comp_container_identifiers.drugbank_id.clone(),
        }
    }
}

/// CCD IDs are uppercase. The RCSB APIs are case-insensitive for them, but we normalize anyway,
/// for URLs and comparisons.
fn ccd_ident(ident: &str) -> String {
    ident.trim().to_uppercase()
}

/// Our agent doesn't treat error status codes as errors, and the RCSB file and data servers
/// return an error body (HTML or JSON) on failure. Catch that here, so we don't hand a failure
/// message back to the caller as if it were data.
fn get_checked(url: &str) -> Result<String, ReqError> {
    let agent = make_agent();
    let mut resp = agent.get(url).call()?;

    if resp.status() != StatusCode::OK {
        return Err(ReqError::Http);
    }

    Ok(resp.body_mut().read_to_string()?)
}

/// Run a search, returning the identifiers of its hits. The Search API answers a query with no
/// hits with an empty 204 response, which we return as an empty list.
fn search_idents(payload: &PdbPayloadSearch, timeout: u64) -> Result<Vec<String>, ReqError> {
    let payload_json = serde_json::to_string(payload)?;

    let agent = make_agent_with_timeout(timeout);
    let mut resp = agent
        .post(SEARCH_API_URL)
        .header("Content-Type", "application/json")
        .send(&payload_json)?;

    match resp.status() {
        StatusCode::NO_CONTENT => return Ok(Vec::new()),
        StatusCode::OK => (),
        _ => return Err(ReqError::Http),
    }

    let parsed: PdbSearchResults = serde_json::from_str(&resp.body_mut().read_to_string()?)?;

    Ok(parsed
        .result_set
        .into_iter()
        .map(|r| r.identifier)
        .collect())
}

/// Find CCD IDs whose chemical attribute exactly matches a value, e.g. an InChIKey or an
/// accession code. See [chemical search attributes](https://search.rcsb.org/chemical-search-attributes.html).
fn ccd_ids_from_attribute(attribute: &str, value: &str) -> Result<Vec<String>, ReqError> {
    let payload = PdbPayloadSearch {
        return_type: ReturnType::MolDefinition,
        query: PdbSearchQuery {
            type_: RcsbType::Terminal,
            service: Service::TextChem,
            parameters: PdbSearchParams {
                attribute: Some(attribute.to_owned()),
                operator: Some(Operator::ExactMatch),
                value: Some(value.to_owned()),
                ..Default::default()
            },
        },
        request_options: Some(SearchRequestOptions {
            return_all_hits: Some(true),
            ..Default::default()
        }),
        ..Default::default()
    };

    search_idents(&payload, SEARCH_TIMEOUT)
}

/// Open the RCSB ligand page for a CCD entry, e.g. https://www.rcsb.org/ligand/MF8
pub fn open_ccd_overview(ident: &str) {
    if let Err(e) = webbrowser::open(&format!("{LIGAND_PAGE_URL}/{}", ccd_ident(ident))) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Load the full CCD entry for a chemical component, e.g. "MF8", from the RCSB Data API.
pub fn load_chem_comp(ident: &str) -> Result<ChemComp, ReqError> {
    let url = format!("{CHEM_COMP_API_URL}/{}", ccd_ident(ident));

    Ok(serde_json::from_str(&get_checked(&url)?)?)
}

/// A curated subset of a CCD entry. See `load_chem_comp` for everything the RCSB has.
///
/// This makes an additional request to ChEBI if RCSB maps the entry to more than one ChEBI id.
/// See `chebi_id_from_ccd`.
pub fn ccd_properties(ident: &str) -> Result<CcdProperties, ReqError> {
    let chem_comp = load_chem_comp(ident)?;

    let mut result: CcdProperties = (&chem_comp).into();
    result.chebi_id = best_chebi_id(&chem_comp)?;

    Ok(result)
}

/// Get the Simplified Molecular Input Line Entry System (SMILES) representation of a CCD entry,
/// including stereochemistry.
pub fn get_ccd_smiles(ident: &str) -> Result<String, ReqError> {
    let d = load_chem_comp(ident)?.rcsb_chem_comp_descriptor;

    d.smiles_stereo.or(d.smiles).ok_or(ReqError::Deserialize)
}

/// Download the CCD definition of a chemical component as an mmCIF string. This is the
/// authoritative definition: it contains atom names (matching those used in PDB entries),
/// elements, charges, and bond orders, and two sets of coordinates: ideal (computed) ones, and
/// model ones taken from an example PDB entry.
pub fn load_ccd_cif(ident: &str) -> Result<String, ReqError> {
    get_checked(&format!("{LIGAND_FILE_URL}/{}.cif", ccd_ident(ident)))
}

/// Download a CCD entry as an SDF string, with ideal (computed) 3D coordinates, and hydrogens.
/// For the coordinates as observed in a PDB entry, use `load_ccd_cif`.
pub fn load_ccd_sdf(ident: &str) -> Result<String, ReqError> {
    get_checked(&format!("{LIGAND_FILE_URL}/{}_ideal.sdf", ccd_ident(ident)))
}

/// Search CCD entries by name, including synonyms and systematic names, e.g. "caffeine", or
/// "metformin". Returns up to 25 CCD IDs. Analogous to `pubchem::find_cids_from_search`.
pub fn find_ccd_ids_from_search(name: &str) -> Result<Vec<String>, ReqError> {
    let payload = PdbPayloadSearch {
        return_type: ReturnType::MolDefinition,
        query: PdbSearchQuery {
            type_: RcsbType::Terminal,
            service: Service::TextChem,
            parameters: PdbSearchParams {
                // Synonyms include the preferred name (`chem_comp.name`), and systematic names.
                attribute: Some("rcsb_chem_comp_synonyms.name".to_owned()),
                operator: Some(Operator::ContainsWords),
                value: Some(name.to_owned()),
                ..Default::default()
            },
        },
        request_options: Some(SearchRequestOptions {
            paginate: Some(Paginate {
                start: 0,
                rows: CCD_SEARCH_ROWS,
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    search_idents(&payload, SEARCH_TIMEOUT)
}

/// Search CCD entries by structure, from a SMILES or InChI string. Returns up to 25 CCD IDs.
/// Analogous to `chebi::structure_search`.
pub fn ccd_structure_search(
    descriptor: &str,
    descriptor_type: DescriptorType,
    match_type: ChemMatchType,
) -> Result<Vec<String>, ReqError> {
    let payload = PdbPayloadSearch {
        return_type: ReturnType::MolDefinition,
        query: PdbSearchQuery {
            type_: RcsbType::Terminal,
            service: Service::Chemical,
            parameters: PdbSearchParams {
                value: Some(descriptor.to_owned()),
                chem_query_type: Some("descriptor".to_owned()),
                descriptor_type: Some(descriptor_type),
                match_type: Some(match_type),
                ..Default::default()
            },
        },
        request_options: Some(SearchRequestOptions {
            paginate: Some(Paginate {
                start: 0,
                rows: CCD_SEARCH_ROWS,
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    search_idents(&payload, CHEM_SEARCH_TIMEOUT)
}

/// Find the CCD entries for a compound from its InChIKey. This is the most general route into
/// the CCD from another database, as it only needs the compound's structure. It's usually a
/// single ID, but can be several, e.g. for duplicate definitions.
pub fn ccd_ids_from_inchi_key(inchi_key: &str) -> Result<Vec<String>, ReqError> {
    ccd_ids_from_attribute("rcsb_chem_comp_descriptor.InChIKey", inchi_key.trim())
}

/// Find the CCD entries for a compound from its PubChem CID, e.g. 2519 (caffeine) -> `["CFF"]`.
/// This uses RCSB's cross-references, which map CCD entries to CIDs by matching InChIKey.
///
/// Returns an empty list if RCSB has no mapping; the CCD only contains compounds observed in (or
/// deposited with) a PDB structure, so most PubChem compounds have no CCD entry.
pub fn ccd_ids_from_pubchem_cid(cid: u32) -> Result<Vec<String>, ReqError> {
    // PubChem is the only resource RCSB cross-references with bare numeric accessions, so this
    // doesn't need to be restricted by `resource_name`.
    ccd_ids_from_attribute(
        "rcsb_chem_comp_related.resource_accession_code",
        &cid.to_string(),
    )
}

/// Find the CCD entries for a compound from its ChEBI id, e.g. 15422 -> `["ATP"]`. This uses
/// RCSB's cross-references, which it takes from PubChem.
///
/// Returns an empty list if RCSB has no mapping. For compounds it's missing, fall back to the
/// InChIKey: `chebi::properties(id)?.inchi_key`, then `ccd_ids_from_inchi_key`.
pub fn ccd_ids_from_chebi_id(id: u32) -> Result<Vec<String>, ReqError> {
    ccd_ids_from_attribute(
        "rcsb_chem_comp_related.resource_accession_code",
        &format!("CHEBI:{id}"),
    )
}

/// Find the PubChem CID of a CCD entry, e.g. "CFF" -> 2519. If RCSB maps several, this returns
/// the lowest; see `ChemComp::pubchem_cids` for all of them. Returns `Ok(None)` if there's no
/// mapping, e.g. for metal ions such as `ZN`, and some cofactors such as `HEM`.
pub fn pubchem_cid_from_ccd(ident: &str) -> Result<Option<u32>, ReqError> {
    Ok(load_chem_comp(ident)?.pubchem_cids().first().copied())
}

/// Find the ChEBI id of a CCD entry, e.g. "CFF" -> 27732. Returns `Ok(None)` if there's no
/// mapping.
///
/// If RCSB maps several (see `ChemComp::chebi_ids`), we load them from ChEBI, and return the
/// lowest whose formula and charge match the CCD entry's. This drops polymers such as glucans,
/// which ChEBI lists under their monomer's InChIKey, and which have formulas like
/// `(C6H10O5)n.H2O`. Among those remaining, e.g. L-alanine and its zwitterion, the lowest id is
/// generally the parent compound.
pub fn chebi_id_from_ccd(ident: &str) -> Result<Option<u32>, ReqError> {
    best_chebi_id(&load_chem_comp(ident)?)
}

/// See `chebi_id_from_ccd`.
fn best_chebi_id(chem_comp: &ChemComp) -> Result<Option<u32>, ReqError> {
    let candidates = chem_comp.chebi_ids();

    if candidates.len() <= 1 {
        return Ok(candidates.first().copied());
    }

    // CCD formulas are space-separated by element, e.g. "C6 H12 O6"; ChEBI's aren't.
    let formula: Option<String> = chem_comp
        .chem_comp
        .formula
        .as_ref()
        .map(|f| f.split_whitespace().collect());
    let charge = chem_comp.chem_comp.pdbx_formal_charge.unwrap_or(0);

    let best = chebi::load_compounds(&candidates)?
        .into_iter()
        .filter(|c| match &c.chemical_data {
            Some(d) => {
                d.formula.is_some() && d.formula == formula && d.charge.unwrap_or(0) == charge
            }
            None => false,
        })
        .map(|c| c.id)
        .min();

    // If nothing matches, e.g. if ChEBI has no formula for these, fall back to the lowest.
    Ok(best.or(candidates.first().copied()))
}

/// Find PDB entries that contain a given CCD component, e.g. structures of proteins bound to a
/// given ligand. This only counts the component as a free (non-polymer) entity, not as a residue
/// in a polymer chain. Returns up to `max_results` PDB IDs; common components such as `HOH` or
/// `SO4` are present in a large fraction of the PDB. Analogous to
/// `pubchem::load_associated_structures`.
pub fn pdb_ids_with_ccd(ident: &str, max_results: u32) -> Result<Vec<String>, ReqError> {
    let payload = PdbPayloadSearch {
        return_type: ReturnType::Entry,
        query: PdbSearchQuery {
            type_: RcsbType::Terminal,
            service: Service::Text,
            parameters: PdbSearchParams {
                attribute: Some(
                    "rcsb_nonpolymer_entity_container_identifiers.nonpolymer_comp_id".to_owned(),
                ),
                operator: Some(Operator::ExactMatch),
                value: Some(ccd_ident(ident)),
                ..Default::default()
            },
        },
        request_options: Some(SearchRequestOptions {
            paginate: Some(Paginate {
                start: 0,
                rows: max_results,
            }),
            ..Default::default()
        }),
        ..Default::default()
    };

    search_idents(&payload, SEARCH_TIMEOUT)
}
