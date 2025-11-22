//! [Home page](https://pubchem.ncbi.nlm.nih.gov/)
//! [API docs](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest)
//!
//! This includes specific lookups, and an interface to the general URL-based API.

use std::{
    fmt::{Display, Formatter},
    io,
    io::ErrorKind,
};

use serde::Deserialize;

use crate::{ReqError, make_agent};

const BASE_COMPOUND_URL: &str = "https://pubchem.ncbi.nlm.nih.gov/compound";

const BASE_PUG_URL: &str = "https://pubchem.ncbi.nlm.nih.gov/rest/pug";

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
    Sdf,
    Cid,
}

impl Display for StructureSearchNamespace {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Smiles => "smiles",
            Self::Inchi => "inchi",
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

fn sdf_url(cid: u32) -> String {
    format!("https://pubchem.ncbi.nlm.nih.gov/rest/pug/compound/cid/{cid}/SDF?record_type=3d",)
}

/// Download an SDF file from PubChem, returning an SDF string.
pub fn load_sdf(cid: u32) -> Result<String, ReqError> {
    let agent = make_agent();

    Ok(agent
        .get(sdf_url(cid))
        .call()?
        .body_mut()
        .read_to_string()?)
}

/// Get the Simplified Molecular Input Line Entry System (SMILES) representation from an identifier.
/// This seems to work using pdbE/Amber identifiers as well as PubChem.
/// todo: Support SELFEIS too; doesn't seem to be available.
pub fn get_smiles(ident: &str) -> Result<String, ReqError> {
    let agent = make_agent();
    let url = format!("https://cactus.nci.nih.gov/chemical/structure/{ident}/smiles");

    // Make sure to catch the HTTP != 200, and return an error: Otherwise the result will be an OK with
    // brief HTML failure message string.
    let mut resp = agent.get(url).call()?;
    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    Ok(resp.body_mut().read_to_string()?)
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
pub fn find_cids_from_search(name: &str) -> Result<Vec<u32>, ReqError> {
    let domain = Domain::Compound; // todo: Compound, Protein, both? Try one then the other?
    let namespace = Namespace::Compound(NamespaceCompound::Name);
    let op_spec = OperationSpecification::Compound(OpSpecCompound::Record);

    let data = url_api_query(domain, namespace, &[name.to_string()], op_spec)?;

    let result: RecordResp = serde_json::from_str(&data)?;

    Ok(result.pc_compounds.iter().map(|p| p.id.id.cid).collect())
}
