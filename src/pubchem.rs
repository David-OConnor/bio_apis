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

use serde::Deserialize;

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
    let idents = identifiers.join(","); // todo: QC the joiner.
    let url = format!("{BASE_PUG_URL}/{domain}/{namespace}/{idents}/{op_spec}/JSON");

    let agent = make_agent();

    Ok(agent.get(url).call()?.body_mut().read_to_string()?)
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
fn sdf_url(id_type: StructureSearchNamespace, id: &str) -> String {
    format!("https://pubchem.ncbi.nlm.nih.gov/rest/pug/compound/{id_type}/{id}/SDF?record_type=3d",)
}

/// Download an SDF file from PubChem, returning an SDF string.
pub fn load_sdf(id_type: StructureSearchNamespace, id: &str) -> Result<String, ReqError> {
    let agent = make_agent();

    Ok(agent
        .get(sdf_url(id_type, id))
        .call()?
        .body_mut()
        .read_to_string()?)
}

/// Get the Simplified Molecular Input Line Entry System (SMILES) representation from an identifier.
/// This seems to work using pdbE/Amber identifiers. Not technically pubchem, but is
/// from NIH.gov.
/// todo: Support SELFEIS too; doesn't seem to be available.
pub fn get_smiles_chem_name(name: &str) -> Result<String, ReqError> {
    let agent = make_agent();
    let url = format!("https://cactus.nci.nih.gov/chemical/structure/{name}/smiles");

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
    // e.g. this is sometimes a problem with SMILES queries.
    let id_santizied = id.replace("#", "%23");

    format!(
        "{BASE_PUG_URL}/compound/{id_type}/{id_santizied}/property/TPSA,XLogP,Complexity,Volume3D,SMILES,InChI,\
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

/// Deserializing only
#[allow(unused)]
#[derive(Debug, Deserialize)]
struct CompoundProps {
    #[serde(rename = "CID")]
    cid: u32,
    // These names match PubChem's PUG-REST property tokens.
    #[serde(rename = "TPSA")]
    tpsa: f32,
    #[serde(rename = "XLogP")]
    xlogp: f32,
    #[serde(rename = "Complexity")]
    complexity: f32,
    #[serde(rename = "Volume3D")]
    volume: f32,
    #[serde(rename = "SMILES")]
    smiles: String,
    #[serde(rename = "InChI")]
    inchi: String,
    #[serde(rename = "InChIKey")]
    inchi_key: String,
    #[serde(rename = "IUPACName")]
    iupac_name: String,
    #[serde(rename = "Title")]
    title: String,
}

/// Deserializing only
#[derive(Debug, Deserialize)]
struct PropertyTableInner {
    #[serde(rename = "Properties")]
    properties: Vec<CompoundProps>,
}

/// Get properties from an ID.
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
}

/// Deserializing only.
#[derive(Debug, Deserialize)]
struct PugViewResp {
    #[serde(rename = "Record")]
    record: PugViewSection,
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
    let smiles = get_smiles_chem_name(pdb_id)?;
    properties(StructureSearchNamespace::Smiles, &smiles)
}

/// We do this via an intermediate SMILES representation.
/// Also returns the SMILES, as we load it anyway.
pub fn get_cid_from_pdbe_id(pdb_id: &str) -> Result<(u32, String), ReqError> {
    let smiles = get_smiles_chem_name(pdb_id)?;
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
