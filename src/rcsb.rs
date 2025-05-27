//! For loading data from the RCSB website's API.

//! For opening the browser to NCBI BLAST, PDB etc.
//!
//! PDB Search API: https://search.rcsb.org/#search-api
//! PDB Data API: https://data.rcsb.org/#data-api

use std::{io, io::Read};

#[cfg(feature = "encode")]
use bincode::{Decode, Encode};
use flate2::read::GzDecoder;
// todo: Determine if you want this.
use na_seq::{AminoAcid, seq_aa_to_str};
use rand::{self, Rng};
use serde::{Deserialize, Serialize, Serializer};
use serde_json::{self};
use ureq::{
    self, Agent, Body,
    http::{Response, StatusCode},
};

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://www.rcsb.org/structure";

const RCSB_3D_VIEW_URL: &str = "https://www.rcsb.org/3d-view";
const STRUCTURE_FILE_URL: &str = "https://files.rcsb.org/view";

const SEARCH_API_URL: &str = "https://search.rcsb.org/rcsbsearch/v2/query";
const DATA_API_URL: &str = "https://data.rcsb.org/rest/v1/core/entry";

// An arbitrary limit to prevent excessive queries to the PDB data api,
// and to simplify display code.
const MAX_RESULTS: usize = 8;

#[derive(Default, Serialize)]
struct PdbSearchParams {
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
}
// "greater", "exact_match", "in", "range", etc. (todo: enum)

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
            Self::NonPolymerEntity => "non_polymer-entity",
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

#[derive(Default, Serialize)]
pub struct PdbSearchQuery {
    /// "terminal", or "group"
    #[serde(rename = "type")]
    type_: RcsbType,
    service: Service,
    parameters: PdbSearchParams,
}

#[derive(Default, Serialize)]
pub struct Sort {
    sort_by: String,
    direction: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    random_seed: Option<u32>,
}

#[derive(Default, Serialize)]
pub struct SearchRequestOptions {
    /// "sequence", "seqmotif", "structmotif", "structure", "chemical", or "text".
    /// Only for sequences?
    // todo: Enum
    #[serde(skip_serializing_if = "Option::is_none")]
    scoring_strategy: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sort: Option<Vec<Sort>>,
    // todo: Paginate
}

#[derive(Default, Serialize)]
pub struct PdbPayloadSearch {
    return_type: ReturnType,
    query: PdbSearchQuery,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_options: Option<SearchRequestOptions>,
    #[serde(skip_serializing_if = "Option::is_none")]
    request_info: Option<String>,
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
    query_id: String,
    result_type: String,
    total_count: u32,
    result_set: Vec<PdbSearchResult>,
}

#[derive(Default, Debug, Deserialize)]
pub struct PdbStruct {
    title: String,
}

#[derive(Default, Debug, Deserialize)]
pub struct PdbDataResults {
    #[serde(rename = "struct")]
    struct_: PdbStruct,
}

#[derive(Default, Debug, Deserialize)]
pub struct PrimaryCitation {
    title: String,
}

#[derive(Default, Debug, Deserialize)]
pub struct PdbMetaDataResults {
    rcsb_primary_citation: PrimaryCitation,
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

    let search_data: PdbSearchResults = serde_json::from_str(&resp).map_err(|_| ReqError {})?;

    if search_data.result_set.is_empty() {
        Err(ReqError {})
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

    let search_data: PdbSearchResults = serde_json::from_str(&resp).map_err(|_| ReqError {})?;

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

        let data: PdbDataResults = serde_json::from_str(&resp).map_err(|_| ReqError {})?;

        result.push(PdbData {
            rcsb_id: r.identifier,
            title: data.struct_.title,
        })
    }

    Ok(result)
}

/// Open a PDB search for this protein's sequence, given a PDB ID, which we load from the API.
pub fn open_overview(ident: &str) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/{ident}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Open a PDB search for this protein's sequence, given a PDB ID, which we load from the API.
pub fn open_3d_view(ident: &str) {
    if let Err(e) = webbrowser::open(&format!("{RCSB_3D_VIEW_URL}/{ident}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Load PDB structure data in the PDBx/mmCIF format. This is a modern, text-based format.
/// It avoids the XML, and limitations of the other two available formats.
pub fn open_structure(ident: &str) {
    let url = format!("{STRUCTURE_FILE_URL}/{ident}.cif");

    if let Err(e) = webbrowser::open(&url) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

pub fn load_metadata(ident: &str) -> Result<PdbMetaData, ReqError> {
    let agent = make_agent();

    let resp = agent
        .get(&format!("{DATA_API_URL}/{}", ident))
        .call()?
        .body_mut()
        .read_to_string()?;

    let data: PdbMetaDataResults = serde_json::from_str(&resp).map_err(|_| ReqError {})?;

    Ok(PdbMetaData {
        prim_cit_title: data.rcsb_primary_citation.title,
    })
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
            io::ErrorKind::InvalidInput,
            "PDB ID must be >= 3 characters.",
        ));
    }

    let ident_middle = &ident[1..3];

    Ok(format!(
        "https://files.rcsb.org/pub/pdb/validation_reports/{}/{ident}/{ident}_validation",
        ident_middle
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

fn structure_factors_cif_url(ident: &str) -> String {
    format!(
        "https://files.rcsb.org/download/{}-sf.cif",
        ident.to_uppercase()
    )
}

fn structure_factors_cif_gz_url(ident: &str) -> String {
    structure_factors_cif_url(ident) + ".gz"
}

fn decode_gz_resp(resp: Response<Body>) -> Result<String, ReqError> {
    let body_reader = resp.into_body().into_reader();
    let mut decoder = GzDecoder::new(body_reader);

    let mut result = String::new();
    decoder.read_to_string(&mut result)?;

    Ok(result)
}

/// Download a (atomic coordinates) mmCIF file (protein atom coords and metadata) from the RCSB,
/// returning an a CIF string. Downloads the compressed (.gz) version, then deocompresses, to save
/// bandwidth.
pub fn load_cif(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    let resp = agent.get(&cif_gz_url(ident)).call()?;
    decode_gz_resp(resp)

    // Ok(agent
    //     .get(cif_url(ident))
    //     .call()?
    //     .body_mut()
    //     .read_to_string()?)
}

/// Download a validation mmCIF file (Related to electron density??) from the RCSB, returning an CIF string.
pub fn load_validation_cif(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    let resp = agent
        .get(&validation_cif_gz_url(ident).unwrap_or_default())
        .call()?;
    decode_gz_resp(resp)
}

/// Download a validation 2fo_fc map mmCIF file (Related to reflections?) from the RCSB, returning an CIF string.
pub fn load_validation_2fo_fc_cif(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    let resp = agent
        .get(&validation_2fo_fc_cif_gz_url(ident).unwrap_or_default())
        .call()?;
    decode_gz_resp(resp)
}

/// Download a validation fo_fc map mmCIF file (related to reflections?) from the RCSB, returning an CIF string.
pub fn load_validation_fo_fc_cif(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    let resp = agent
        .get(&validation_fo_fc_cif_gz_url(ident).unwrap_or_default())
        .call()?;
    decode_gz_resp(resp)
}

/// Download a structure factors (e.g. computed electron density over space) mmCIF file
/// from the RCSB, returning an CIF string.
pub fn load_structure_factors_cif(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    let resp = agent.get(&structure_factors_cif_gz_url(ident)).call()?;
    decode_gz_resp(resp)

    // Ok(agent
    //     .get(structure_factors_cif_url(ident))
    //     .call()?
    //     .body_mut()
    //     .read_to_string()?)
}

#[cfg_attr(feature = "encode", derive(Encode, Decode))]
#[derive(Clone, Debug)]
pub struct DataAvailable {
    pub validation: bool,
    pub validation_2fo_fc: bool,
    pub validation_fo_fc: bool,
    pub structure_factors: bool,
}

fn file_exists(url: &str, agent: &Agent) -> Result<bool, ReqError> {
    Ok(agent.head(url).call()?.status() == StatusCode::OK)
}

/// Find out if additional data files are available, such as structure factors and validation data.
pub fn get_data_avail(ident: &str) -> Result<DataAvailable, ReqError> {
    let agent = make_agent();

    // todo: This is too expensive; find a better way??

    Ok(DataAvailable {
        validation: file_exists(&validation_cif_gz_url(ident).unwrap_or_default(), &agent)?,
        validation_2fo_fc: file_exists(
            &validation_2fo_fc_cif_gz_url(ident).unwrap_or_default(),
            &agent,
        )?,
        validation_fo_fc: file_exists(
            &validation_fo_fc_cif_gz_url(ident).unwrap_or_default(),
            &agent,
        )?,
        // validation: false,
        // validation_2fo_fc: false,
        // validation_fo_fc: false,
        structure_factors: file_exists(&structure_factors_cif_url(ident), &agent)?,
    })
}
