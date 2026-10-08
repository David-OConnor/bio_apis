//! [Home page](https://pubchem.ncbi.nlm.nih.gov/)
//! [API docs](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest)
//!
//! This includes specific lookups, and an interface to the general URL-based API.
//!
//! //! Compared to ChEBI, PubChem is a larger, less-curated database.

use std::{
    collections::HashMap,
    fmt::{Display, Formatter},
};

use serde::{Deserialize, Serialize};

use crate::{ReqError, chebi, make_agent};

const BASE_COMPOUND_URL: &str = "https://pubchem.ncbi.nlm.nih.gov/compound";

const BASE_PUG_URL: &str = "https://pubchem.ncbi.nlm.nih.gov/rest/pug";

const BASE_PUG_VIEW_URL: &str = "https://pubchem.ncbi.nlm.nih.gov/rest/pug_view/data";

const PROTEIN_LOOKUP_URL: &str =
    "https://pubchem.ncbi.nlm.nih.gov/rest/pug_view/structure/compound";

#[allow(unused)]
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

/// https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=The-URL-Path
#[derive(Clone, Copy, PartialEq)]
pub enum Domain {
    Substance,
    Compound,
    Assay,
    Gene,
    Protein,
    Pathway,
    Taxonomy,
    Cell,
}
impl Display for Domain {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Substance => "substance",
            Self::Compound => "compound",
            Self::Assay => "assay",
            Self::Gene => "gene",
            Self::Protein => "protein",
            Self::Pathway => "pathway",
            Self::Taxonomy => "taxonomy",
            Self::Cell => "cell",
        };
        write!(f, "{v}")
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum StructureSearchCat {
    Substructure,
    Superstructure,
    Similarity,
    Identity,
}

impl Display for StructureSearchCat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Substructure => "substructure",
            Self::Superstructure => "superstructure",
            Self::Similarity => "similarity",
            Self::Identity => "identity",
        };
        write!(f, "{v}")
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum FastSearchCat {
    FastIdentity,
    FastSimilarity2d,
    FastSimilarity3d,
    FastSubstructure,
    FastSuperstructure,
}

impl Display for FastSearchCat {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::FastIdentity => "fastidentity",
            Self::FastSimilarity2d => "fastsimilarity_2d",
            Self::FastSimilarity3d => "fastsimilarity_3d",
            Self::FastSubstructure => "fastsubstructure",
            Self::FastSuperstructure => "fastsuperstructure",
        };
        write!(f, "{v}")
    }
}

#[derive(Clone, Copy, PartialEq)]
pub enum StructureSearchNamespace {
    Smiles,
    Inchi,
    InchiKey,
    Sdf,
    Cid,
}

impl Display for StructureSearchNamespace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Smiles => "smiles",
            Self::Inchi => "inchi",
            Self::InchiKey => "inchikey",
            Self::Sdf => "sdf",
            Self::Cid => "cid",
        };
        write!(f, "{v}")
    }
}

/// https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=The-URL-Path
#[derive(Clone, PartialEq)]
pub enum NamespaceCompound {
    Cid,
    Name,
    Smiles,
    Inchi,
    Sdf,
    Inchikey,
    Formula,
    StructureSearch((StructureSearchCat, StructureSearchNamespace)),
    // xrf, // todo
    // mass // todo
    ListKey,
    FastSearch((FastSearchCat, StructureSearchNamespace)),
}
impl Display for NamespaceCompound {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Cid => "cid",
            Self::Name => "name",
            Self::Smiles => "smiles",
            Self::Inchi => "inchi",
            Self::Sdf => "sdf",
            Self::Inchikey => "inchikey",
            Self::Formula => "formula",
            Self::StructureSearch((search_cat, search_namespace)) => {
                &format!("{search_cat}/{search_namespace}")
            }
            Self::ListKey => "listkey",
            Self::FastSearch((search_cat, search_namespace)) => {
                &format!("{search_cat}/{search_namespace}")
            }
        };
        write!(f, "{v}")
    }
}

/// https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=The-URL-Path
#[derive(Clone, PartialEq)]
pub enum NamespaceSubstance {
    Sid,
    SourceId(String),
    SourceAll(String),
    Name,
    // Xref
    ListKey,
}
impl Display for NamespaceSubstance {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Sid => "sid",
            Self::SourceId(v) => &format!("sourceid/{v}"),
            Self::SourceAll(v) => &format!("sourceall/{v}"),
            Self::Name => "name",
            Self::ListKey => "listkey",
        };
        write!(f, "{v}")
    }
}

/// https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=The-URL-Path
#[derive(Clone, PartialEq)]
pub enum Namespace {
    Compound(NamespaceCompound),
    Substance(NamespaceSubstance),
}

impl Display for Namespace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Compound(v) => v.to_string(),
            Self::Substance(v) => v.to_string(),
        };
        write!(f, "{v}")
    }
}

/// https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=The-URL-Path
#[derive(Clone, PartialEq)]
pub enum OpSpecCompound {
    Record,
    Property(Vec<String>),
    Synonyms,
    Sids,
    Cids,
    Aids,
    AssaySummary,
    Classification,
    Xrefs,
    Description,
    Conformers,
}

impl Display for OpSpecCompound {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Record => "record",
            Self::Property(v) => &format!("property/{}", v.join(",")),
            Self::Synonyms => "synonyms",
            Self::Sids => "sids",
            Self::Cids => "cids",
            Self::Aids => "aids",
            Self::AssaySummary => "assaysummary",
            Self::Classification => "classification",
            Self::Xrefs => "xrefs",
            Self::Description => "description",
            Self::Conformers => "conformers",
        };
        write!(f, "{v}")
    }
}

/// https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=The-URL-Path
#[derive(Clone, Copy, PartialEq)]
pub enum OpSpecSubstance {
    Record,
    Synonyms,
    Sids,
    Cids,
    Aids,
    AssaySummary,
    Classification,
    Xrefs,
    Description,
}

impl Display for OpSpecSubstance {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Record => "record",
            Self::Synonyms => "synonyms",
            Self::Sids => "sids",
            Self::Cids => "cids",
            Self::Aids => "aids",
            Self::AssaySummary => "assaysummary",
            Self::Classification => "classification",
            Self::Xrefs => "xrefs",
            Self::Description => "description",
        };
        write!(f, "{v}")
    }
}

/// https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=The-URL-Path
#[derive(Clone, PartialEq)]
pub enum OperationSpecification {
    Substance(OpSpecSubstance),
    Compound(OpSpecCompound),
}

impl Display for OperationSpecification {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Substance(v) => v.to_string(),
            Self::Compound(v) => v.to_string(),
        };
        write!(f, "{v}")
    }
}

/// Calls the flexible [URL-based API](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=URL-based-API).
/// Makes GET requests by combining parameters. Returns JSON results.
///
/// The top-level query structure: `https://pubchem.ncbi.nlm.nih.gov/rest/pug/<input specification>/<operation specification>/[<output specification>][?<operation_options>]`
/// Note: The documentation is a bit tough to understand in parts; we have room for improvement.
pub fn url_api_query(
    domain: Domain,
    namespace: Namespace,
    identifiers: &[String],
    op_spec: OperationSpecification,
    // op_options, Vec<Operation> // todo
    // todo: String output for now.
) -> Result<String, ReqError> {
    // todo: Op options
    let idents = identifiers
        .iter()
        .map(|ident| encode_path_segment(ident))
        .collect::<Vec<_>>()
        .join(",");
    let url = format!("{BASE_PUG_URL}/{domain}/{namespace}/{idents}/{op_spec}/JSON");

    let agent = make_agent();

    Ok(agent.get(url).call()?.body_mut().read_to_string()?)
}

/// Percent-encode user or database text before placing it in one segment of a PUG-REST URL path.
/// This is required for names containing spaces and for structure strings containing reserved
/// characters such as `+`, `#`, `/`, and `?`.
fn encode_path_segment(value: &str) -> String {
    let mut url = url::Url::parse("https://example.invalid/")
        .expect("the static path-encoding URL must be valid");
    url.path_segments_mut()
        .expect("the static URL must support path segments")
        .push(value);
    url.path().trim_start_matches('/').to_owned()
}

#[derive(Clone, Debug, Deserialize)]
struct SimilarMolsCidResp {
    #[serde(rename = "CID")]
    pub cid: Vec<u32>,
}

#[derive(Clone, Debug, Deserialize)]
/// For decoding
struct SimilarMolsResp {
    #[serde(rename = "IdentifierList")]
    pub identifier_list: SimilarMolsCidResp,
}

/// Find similar molecules using the fast 3D lookup.
// todo: Expose in bio_files or here your Ident enum, and pass that here instead of requiring CID
// todo: You will eventually need to do this using SMILES, for compatibility with custom molecules.
// pub fn find_similar_mols(cid: u32) -> Result<Vec<String>, ReqError> {
pub fn find_similar_mols(cid: u32) -> Result<Vec<u32>, ReqError> {
    let resp = url_api_query(
        Domain::Compound,
        Namespace::Compound(NamespaceCompound::FastSearch((
            FastSearchCat::FastSimilarity3d,
            StructureSearchNamespace::Cid,
        ))),
        &[cid.to_string()],
        OperationSpecification::Compound(OpSpecCompound::Cids),
    )?;

    let parsed: SimilarMolsResp = serde_json::from_str(&resp)?;
    Ok(parsed.identifier_list.cid)
}

pub fn open_overview(id: u32) {
    if let Err(e) = webbrowser::open(&format!("{BASE_COMPOUND_URL}/{id}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Find proteins associated with this small organic molecule, e.g. if it's a ligand,
/// which proteins it can bind to. This notably includes PDB urls
pub fn load_associated_structures(cid: u32) -> Result<Vec<ProteinStructure>, ReqError> {
    let url = format!("{PROTEIN_LOOKUP_URL}/{cid}/JSON");
    let agent = make_agent();

    let resp = agent.get(url).call()?.body_mut().read_to_string()?;

    let parsed: ProteinStructureResponse = serde_json::from_str(&resp)?;
    Ok(parsed.structure.structures)
}

/// Note: If id is a u32 CID`, convert to str prior to passing here.
/// `record_type` is `"3d"` or `"2d"`.
fn sdf_url(id_type: StructureSearchNamespace, id: &str, record_type: &str) -> String {
    let id = encode_path_segment(id);

    format!(
        "https://pubchem.ncbi.nlm.nih.gov/rest/pug/compound/{id_type}/{id}/SDF?record_type={record_type}",
    )
}

/// Download an SDF file from PubChem, returning an SDF string. Uses the 3D conformer if available;
/// falls back to the 2D (Z = 0) record otherwise.
pub fn load_sdf(id_type: StructureSearchNamespace, id: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    for record_type in ["3d", "2d"] {
        // Our agent doesn't treat HTTP error codes as errors, so check the status explicitly:
        // PubChem returns a 404 with a plain-text fault body when e.g. no 3D record exists.
        let mut resp = agent.get(sdf_url(id_type, id, record_type)).call()?;
        if resp.status() == 200 {
            return Ok(resp.body_mut().read_to_string()?);
        }
    }

    Err(ReqError::Http)
}

/// Get the Simplified Molecular Input Line Entry System (SMILES) representation from an identifier.
/// This seems to work using pdbE/Amber identifiers. Not technically pubchem, but is
/// from NIH.gov.
/// todo: Support SELFEIS too; doesn't seem to be available.
pub fn get_smiles_from_pdbe_id(pdbe_ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();
    let url = format!("https://cactus.nci.nih.gov/chemical/structure/{pdbe_ident}/smiles");

    // Make sure to catch the HTTP != 200, and return an error: Otherwise the result will be an OK with
    // brief HTML failure message string.
    let mut resp = agent.get(url).call()?;

    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    Ok(resp.body_mut().read_to_string()?)
}

fn pubchem_smiles_url(cid: u32) -> String {
    format!("{BASE_PUG_URL}/compound/cid/{cid}/property/IsomericSMILES/TXT")
}

/// Get SMILES directly from a PubChem CID via PUG-REST.
pub fn get_smiles(cid: u32) -> Result<String, ReqError> {
    let agent = make_agent();
    let url = pubchem_smiles_url(cid);

    let mut resp = agent.get(url).call()?;
    let s = resp.body_mut().read_to_string()?;
    Ok(s.trim().to_string())
}

/// Todo: You could make this more generic.
fn properties_url(id_type: StructureSearchNamespace, id: &str) -> String {
    let id_sanitized = encode_path_segment(id);

    format!(
        "{BASE_PUG_URL}/compound/{id_type}/{id_sanitized}/property/TPSA,XLogP,Complexity,Volume3D,SMILES,InChI,\
    InChIKey,IUPACName,Title/JSON"
    )
}

/// This is currently a curated set for a specific application in Molchanica.
/// [Properties list](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=Compound-Property-Tables)
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct Properties {
    /// Computationally generated octanol-water partition coefficient or distribution coefficient.
    /// XLogP is used as a measure of hydrophilicity or hydrophobicity of a molecule.
    pub log_p: f32,
    pub total_polar_surface_area: f32,
    /// The molecular complexity rating of a compound, computed using the Bertz/Hendrickson/Ihlenfeldt formula.
    pub complexity: f32,
    /// Analytic volume of the first diverse conformer (default conformer) for a compound.
    pub volume: f32,
    /// E.g., if loaded from SMILES or some other query, that's not a CID.
    pub cid: u32,
    /// A SMILES (Simplified Molecular Input Line Entry System) string, which includes both stereochemical and isotopic information. See the glossary entry on SMILES for more detail.
    pub smiles: String,
    /// Standard IUPAC International Chemical Identifier (InChI). It does not allow for user
    /// selectable options in dealing with the stereochemistry and tautomer layers of the InChI string.
    pub inchi: String,
    /// Hashed version of the full standard InChI, consisting of 27 characters.
    pub inchi_key: String,
    /// Chemical name systematically determined according to the IUPAC nomenclatures.
    pub iupac_name: String,
    /// The title used for the compound summary page.
    pub title: String,
}

/// Deserializing only
#[derive(Debug, Deserialize)]
struct PropertyTableResp {
    #[serde(rename = "PropertyTable")]
    property_table: PropertyTableInner,
}

/// Deserializing only. Only the CID is required: PubChem omits properties it doesn't have for a
/// compound, and answers a structure query it has no compound for with only `"CID": 0`.
#[allow(unused)]
#[derive(Debug, Deserialize)]
struct CompoundProps {
    #[serde(rename = "CID")]
    cid: u32,
    // These names match PubChem's PUG-REST property tokens.
    #[serde(rename = "TPSA", default)]
    tpsa: f32,
    #[serde(rename = "XLogP", default)]
    xlogp: f32,
    #[serde(rename = "Complexity", default)]
    complexity: f32,
    #[serde(rename = "Volume3D", default)]
    volume: f32,
    #[serde(rename = "SMILES", default)]
    smiles: String,
    #[serde(rename = "InChI", default)]
    inchi: String,
    #[serde(rename = "InChIKey", default)]
    inchi_key: String,
    #[serde(rename = "IUPACName", default)]
    iupac_name: String,
    #[serde(rename = "Title", default)]
    title: String,
}

/// Deserializing only
#[derive(Debug, Deserialize)]
struct PropertyTableInner {
    #[serde(rename = "Properties")]
    properties: Vec<CompoundProps>,
}

/// Get properties from an ID.
/// See [Compound Property Tables](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=Compound-Property-Tables)
/// for a list of supported fields. This function chooses a subset.
pub fn properties(id_type: StructureSearchNamespace, id: &str) -> Result<Properties, ReqError> {
    let agent = make_agent();
    let url = properties_url(id_type, id);

    let mut resp = agent.get(url).call()?;
    let body = resp.body_mut().read_to_string()?;

    let parsed: PropertyTableResp = serde_json::from_str(&body)?;

    let row = parsed
        .property_table
        .properties
        .into_iter()
        .next()
        .ok_or(ReqError::Deserialize)?;

    Ok(Properties {
        log_p: row.xlogp,
        total_polar_surface_area: row.tpsa,
        complexity: row.complexity,
        volume: row.volume,
        cid: row.cid,
        smiles: row.smiles,
        inchi: row.inchi,
        inchi_key: row.inchi_key,
        iupac_name: row.iupac_name,
        title: row.title,
    })
}

/// Deserializing only; one row of a CID -> Title property lookup.
#[derive(Debug, Deserialize)]
struct TitleRow {
    #[serde(rename = "CID")]
    cid: u32,
    #[serde(rename = "Title")]
    title: Option<String>,
}

/// Deserializing only.
#[derive(Debug, Deserialize)]
struct TitleTableInner {
    #[serde(rename = "Properties")]
    properties: Vec<TitleRow>,
}

/// Deserializing only.
#[derive(Debug, Deserialize)]
struct TitleTableResp {
    #[serde(rename = "PropertyTable")]
    property_table: TitleTableInner,
}

/// Fetch PubChem compound titles for many CIDs in a single request, keyed by CID. PubChem's
/// property endpoint accepts a comma-separated list of CIDs, so this collapses what would otherwise
/// be one request per molecule into one.
///
/// The caller should chunk very large lists (a long request URL can be rejected) and rate-limit
/// between calls. CIDs PubChem has no title for are simply absent from the returned map.
pub fn titles_for_cids(cids: &[u32]) -> Result<HashMap<u32, String>, ReqError> {
    if cids.is_empty() {
        return Ok(HashMap::new());
    }

    let idents: Vec<String> = cids.iter().map(|c| c.to_string()).collect();

    let data = url_api_query(
        Domain::Compound,
        Namespace::Compound(NamespaceCompound::Cid),
        &idents,
        OperationSpecification::Compound(OpSpecCompound::Property(vec!["Title".to_string()])),
    )?;

    let parsed: TitleTableResp = serde_json::from_str(&data)?;

    Ok(parsed
        .property_table
        .properties
        .into_iter()
        .filter_map(|row| row.title.map(|t| (row.cid, t)))
        .collect())
}

/// Deserializing only; PUG-View nests sections to a depth that varies by heading, so this is
/// recursive. `Record` itself deserializes as a section itself, as it carries the outermost
/// `Section` list.
#[derive(Debug, Deserialize)]
struct PugViewSection {
    #[serde(rename = "TOCHeading", default)]
    heading: String,
    #[serde(rename = "Section", default)]
    sections: Vec<PugViewSection>,
    #[serde(rename = "Information", default)]
    information: Vec<PugViewInfo>,
}

impl PugViewSection {
    /// The first information string anywhere in this subtree. Requests are filtered by heading, so
    /// any value present is one we asked for.
    fn first_value(&self) -> Option<&str> {
        for info in &self.information {
            if let Some(s) = info.value.strings.first() {
                return Some(&s.value);
            }
        }

        self.sections.iter().find_map(|s| s.first_value())
    }
}

/// Deserializing only.
#[derive(Debug, Deserialize)]
struct PugViewInfo {
    #[serde(rename = "Name", default)]
    name: String,
    #[serde(rename = "ReferenceNumber", default)]
    reference_number: Option<u32>,
    #[serde(rename = "Value")]
    value: PugViewValue,
}

/// Deserializing only.
#[derive(Debug, Deserialize)]
struct PugViewValue {
    #[serde(rename = "StringWithMarkup", default)]
    strings: Vec<PugViewString>,
}

/// Deserializing only.
#[derive(Debug, Deserialize)]
struct PugViewString {
    #[serde(rename = "String")]
    value: String,
    #[serde(rename = "Markup", default)]
    markup: Vec<PugViewMarkup>,
}

#[derive(Debug, Deserialize)]
struct PugViewMarkup {
    #[serde(rename = "URL", default)]
    url: String,
    #[serde(rename = "Extra", default)]
    extra: String,
}

/// Deserializing only.
#[derive(Debug, Deserialize)]
struct PugViewResp {
    #[serde(rename = "Record")]
    record: PugViewSection,
}

/// A classification contributor, as attributed by PubChem. The subject can identify a
/// particular form or mixture; contributors do not necessarily classify the same formulation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct SafetySource {
    #[serde(alias = "SourceName")]
    pub name: String,
    #[serde(alias = "Name", default)]
    pub subject: String,
    #[serde(alias = "URL", default)]
    pub url: String,
}

/// Reported GHS pictograms, aggregated across PubChem's classification contributors.
/// False means that a pictogram was not reported, not that the compound is safe. These
/// describe material hazards, not reaction risk, exposure, or a supplier's formulation.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct SafetyData {
    /// GHS01: explosives, some self-reactives and organic peroxides.
    pub explosive: bool,
    /// GHS02: flammables, pyrophorics, self-heating and related fire hazards.
    pub flammable: bool,
    /// GHS03: oxidizers.
    pub oxidizing: bool,
    /// GHS04: gas under pressure.
    pub gas_under_pressure: bool,
    /// GHS05: skin corrosion, serious eye damage, or corrosion of metals.
    pub corrosive: bool,
    /// GHS06: acute toxicity (fatal or toxic). Harmful acute toxicity uses GHS07.
    pub acute_toxicity: bool,
    /// GHS07: irritation, skin sensitization, harmful acute toxicity, or narcotic effects.
    pub irritant: bool,
    /// GHS08: carcinogenicity, mutagenicity, reproductive/organ toxicity, respiratory
    /// sensitization, or aspiration hazard.
    pub health_hazard: bool,
    /// GHS09: aquatic environmental hazards.
    pub environmental_hazard: bool,
    /// Strongest reported signal word (Danger takes precedence over Warning).
    pub signal_word: Option<String>,
    /// Original H-code statements, including classification and reporting-percentage notes.
    pub hazard_statements: Vec<String>,
    pub sources: Vec<SafetySource>,
    pub pubchem_url: String,
    /// Date of a checked-in snapshot, if supplied by the caller (YYYY-MM-DD).
    pub retrieved_on: Option<String>,
}

impl SafetyData {
    /// Stable GHS code and readable label for each reported pictogram.
    pub fn pictograms(&self) -> Vec<(&'static str, &'static str)> {
        [
            (self.explosive, "GHS01", "Explosive"),
            (self.flammable, "GHS02", "Flammable"),
            (self.oxidizing, "GHS03", "Oxidizing"),
            (self.gas_under_pressure, "GHS04", "Gas under pressure"),
            (self.corrosive, "GHS05", "Corrosive / eye damage"),
            (self.acute_toxicity, "GHS06", "Acute toxicity (fatal/toxic)"),
            (self.irritant, "GHS07", "Irritant / harmful"),
            (self.health_hazard, "GHS08", "Health hazard"),
            (self.environmental_hazard, "GHS09", "Environmental hazard"),
        ]
        .into_iter()
        .filter_map(|(reported, code, label)| reported.then_some((code, label)))
        .collect()
    }

    fn add_pictogram(&mut self, markup: &PugViewMarkup) -> bool {
        let filename = markup.url.rsplit('/').next().unwrap_or_default();
        let code = filename.split('.').next().unwrap_or_default();
        let flag = match (code, markup.extra.as_str()) {
            ("GHS01", _) | (_, "Explosive") => &mut self.explosive,
            ("GHS02", _) | (_, "Flammable") => &mut self.flammable,
            ("GHS03", _) | (_, "Oxidizer" | "Oxidizing") => &mut self.oxidizing,
            ("GHS04", _) | (_, "Compressed Gas" | "Gas Under Pressure") => {
                &mut self.gas_under_pressure
            }
            ("GHS05", _) | (_, "Corrosive") => &mut self.corrosive,
            ("GHS06", _) | (_, "Acute Toxic" | "Acute Toxicity") => &mut self.acute_toxicity,
            ("GHS07", _) | (_, "Irritant") => &mut self.irritant,
            ("GHS08", _) | (_, "Health Hazard") => &mut self.health_hazard,
            ("GHS09", _) | (_, "Environmental Hazard") => &mut self.environmental_hazard,
            _ => return false,
        };
        *flag = true;
        true
    }
}

#[derive(Debug, Deserialize)]
struct PugViewSafetyReference {
    #[serde(rename = "ReferenceNumber")]
    number: u32,
    #[serde(flatten)]
    source: SafetySource,
}

#[derive(Debug, Deserialize)]
struct PugViewSafetyRecord {
    #[serde(rename = "RecordNumber")]
    cid: u32,
    #[serde(flatten)]
    section: PugViewSection,
    #[serde(rename = "Reference", default)]
    references: Vec<PugViewSafetyReference>,
}

#[derive(Debug, Deserialize)]
struct PugViewSafetyResp {
    #[serde(rename = "Record")]
    record: PugViewSafetyRecord,
}

fn collect_safety(
    section: &PugViewSection,
    in_ghs: bool,
    data: &mut SafetyData,
    references: &mut Vec<u32>,
) -> bool {
    let in_ghs = in_ghs || section.heading == "GHS Classification";
    let mut found = false;

    if in_ghs {
        for info in &section.information {
            let mut reported = false;
            for value in &info.value.strings {
                match info.name.as_str() {
                    "Pictogram(s)" => {
                        for markup in &value.markup {
                            reported |= data.add_pictogram(markup);
                        }
                    }
                    "Signal" => {
                        let signal = value.value.trim();
                        if signal == "Danger" || signal == "Warning" {
                            if signal == "Danger" || data.signal_word.is_none() {
                                data.signal_word = Some(signal.to_owned());
                            }
                            reported = true;
                        }
                    }
                    "GHS Hazard Statements" => {
                        let statement = value.value.trim();
                        if !statement.is_empty() {
                            if !data.hazard_statements.iter().any(|s| s == statement) {
                                data.hazard_statements.push(statement.to_owned());
                            }
                            reported = true;
                        }
                    }
                    _ => {}
                }
            }

            if reported {
                found = true;
                if let Some(number) = info.reference_number
                    && !references.contains(&number)
                {
                    references.push(number);
                }
            }
        }
    }

    for child in &section.sections {
        found |= collect_safety(child, in_ghs, data, references);
    }
    found
}

fn parse_safety_data(cid: u32, body: &str) -> Result<Option<SafetyData>, ReqError> {
    let parsed: PugViewSafetyResp = serde_json::from_str(body)?;
    if parsed.record.cid != cid {
        return Err(ReqError::Deserialize);
    }

    let mut data = SafetyData {
        pubchem_url: format!("{BASE_COMPOUND_URL}/{cid}#section=GHS-Classification"),
        ..SafetyData::default()
    };
    let mut references = Vec::new();
    if !collect_safety(&parsed.record.section, false, &mut data, &mut references) {
        return Ok(None);
    }

    data.sources = parsed
        .record
        .references
        .into_iter()
        .filter(|reference| references.contains(&reference.number))
        .map(|reference| reference.source)
        .collect();
    Ok(Some(data))
}

/// Retrieve GHS safety annotations through [PUG-View](https://pubchem.ncbi.nlm.nih.gov/pug_view/).
/// Returns `None` when no usable GHS classification is available, including a 404. Network,
/// HTTP and malformed-response failures remain errors. Calls must respect PubChem's limit
/// of five requests per second; cache results when browsing many compounds.
pub fn safety_data(cid: u32) -> Result<Option<SafetyData>, ReqError> {
    let agent = make_agent();
    let url = format!("{BASE_PUG_VIEW_URL}/compound/{cid}/JSON?heading=GHS+Classification");
    let mut resp = agent.get(url).call()?;

    if resp.status() == 404 {
        return Ok(None);
    }
    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    parse_safety_data(cid, &resp.body_mut().read_to_string()?)
}

/// Find the ChEBI id of a compound from its PubChem CID, e.g. 2519 (caffeine) -> 27732.
///
/// ChEBI records don't cross-reference PubChem, so PubChem is the only side carrying this link; it
/// has it because ChEBI deposits its entries into PubChem. We use PUG-View's `ChEBI ID` heading,
/// which serves the single curated accession in a small response. (The `synonyms` and
/// `xrefs/RegistryID` operations also expose it, but synonyms is ambiguous — CID 5793 lists three
/// ChEBI ids, unranked — and xrefs buries it in thousands of unrelated registry ids.)
///
/// Returns `Ok(None)` if PubChem has no ChEBI id for the compound; this also covers a CID that
/// doesn't exist, as PubChem answers both with a 404. For compounds ChEBI hasn't deposited, fall
/// back to a structure lookup: pass this compound's InChI key to `chebi::search`, or its SMILES to
/// `chebi::structure_search`.
///
/// Note that PubChem rate limits to 5 requests/second, so mapping many compounds this way needs
/// throttling.
pub fn chebi_id_from_cid(cid: u32) -> Result<Option<u32>, ReqError> {
    let agent = make_agent();
    let url = format!("{BASE_PUG_VIEW_URL}/compound/{cid}/JSON?heading=ChEBI+ID");

    // Our agent doesn't treat error status codes as errors, and PUG-View returns a JSON body on
    // failure, e.g. `{"Fault": {"Code": "PUGVIEW.NotFound", ...}}`. Catch that here, so we don't
    // try to parse a failure message as data.
    let mut resp = agent.get(url).call()?;

    if resp.status() == 404 {
        return Ok(None);
    }

    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    let parsed: PugViewResp = serde_json::from_str(&resp.body_mut().read_to_string()?)?;

    match parsed.record.first_value() {
        // The prefixed form, e.g. "CHEBI:27732".
        Some(v) => Ok(Some(chebi::parse_id(v)?)),
        None => Ok(None),
    }
}

pub fn properties_from_pdbe_id(pdb_id: &str) -> Result<Properties, ReqError> {
    let smiles = get_smiles_from_pdbe_id(pdb_id)?;
    properties(StructureSearchNamespace::Smiles, &smiles)
}

/// We do this via an intermediate SMILES representation.
/// Also returns the SMILES, as we load it anyway.
pub fn get_cid_from_pdbe_id(pdb_id: &str) -> Result<(u32, String), ReqError> {
    let smiles = get_smiles_from_pdbe_id(pdb_id)?;
    let cids = find_cids_from_search(&smiles, true)?;

    Ok((cids[0], smiles))
}

#[allow(unused)]
#[derive(Clone, Debug, Deserialize)]
struct RecordIdB {
    cid: u32,
}

#[allow(unused)]
#[derive(Clone, Debug, Deserialize)]
struct RecordIdA {
    id: RecordIdB,
}

#[allow(unused)]
#[derive(Clone, Debug, Deserialize)]
struct PcCompound {
    id: RecordIdA,
    // todo: Other fields A/R.
    // atoms: Vec<u32>,
}

#[allow(unused)]
#[derive(Clone, Debug, Deserialize)]
struct RecordResp {
    #[serde(rename = "PC_Compounds")]
    pc_compounds: Vec<PcCompound>,
}

/// Load a list of CIDs from a name search
pub fn find_cids_from_search(name: &str, smiles: bool) -> Result<Vec<u32>, ReqError> {
    let domain = Domain::Compound; // todo: Compound, Protein, both? Try one then the other?

    let nsc = if smiles {
        NamespaceCompound::Smiles
    } else {
        NamespaceCompound::Name
    };
    let namespace = Namespace::Compound(nsc);

    let op_spec = OperationSpecification::Compound(OpSpecCompound::Record);

    let data = url_api_query(domain, namespace, &[name.to_string()], op_spec)?;

    let result: RecordResp = serde_json::from_str(&data)?;

    Ok(result.pc_compounds.iter().map(|p| p.id.id.cid).collect())
}
