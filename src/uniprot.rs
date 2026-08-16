//! [Home page](https://www.uniprot.org/)
//! [API docs](https://www.uniprot.org/help/api)
//! [Query syntax](https://www.uniprot.org/help/query-fields)
//!
//! UniProt is the reference knowledgebase of protein sequence and function. Unlike PubChem, ChEBI
//! and DrugBank, its entries are *proteins* rather than small molecules; this module covers
//! UniProtKB, which is split into the manually curated Swiss-Prot and the automatically annotated
//! TrEMBL.
//!
//! This is a good option compard to RSCB/PDB when identifying a proteins' function or sequence,
//! including RHEA interaction. Use RCSB for its 3D structure.
//!
//! Identifiers here are UniProtKB accessions, e.g. `P69905`. Functions accept either the bare
//! accession, or one prefixed with the database name (`UniProtKB:P69905`, case-insensitive).
//! Isoform suffixes, e.g. `P69905-1`, are preserved.
//!
//! A full record is a large download — a well-studied protein can exceed 250kB of JSON, most of it
//! cross-references. Pass a `Field` list to `load_protein_fields` or `search` to ask for only the
//! parts you need; `properties` does this for you.
//!
//! Bridges to the other modules in this crate: `Protein::pdb_ids` feeds `rcsb`,
//! `Protein::rhea_ids` feeds `rhea`, and `Protein::chebi_ids` feeds `chebi`.
//!
//! Note: Predicted structures come from AlphaFold DB (alphafold.ebi.ac.uk) rather than UniProt
//! itself, but are keyed by UniProt accession, so we serve them here.
//!
//! Note: UniProt asks that programs identify themselves via the User-Agent header, so we set one.

use std::{
    fmt::{Display, Formatter},
    thread,
    time::Duration,
};

use na_seq::{AminoAcid, seq_aa_from_str};
use serde::Deserialize;

use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://www.uniprot.org/uniprotkb";

const API_URL: &str = "https://rest.uniprot.org/uniprotkb";
const ID_MAPPING_URL: &str = "https://rest.uniprot.org/idmapping";

/// AlphaFold DB; keyed by UniProt accession. See the note in the module docs.
const ALPHAFOLD_URL: &str = "https://alphafold.ebi.ac.uk";

/// UniProt's per-page maximum on its search endpoints.
const PAGE_SIZE_MAX: u32 = 500;

/// How long to wait for an ID-mapping job, and how often to check on it.
const JOB_POLL_INTERVAL: Duration = Duration::from_millis(500);
const JOB_POLL_ATTEMPTS: u32 = 40;

const USER_AGENT: &str = concat!(
    "bio_apis/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/David-OConnor/bio_apis)"
);

/// The fields a response can carry. This is a curated subset of the
/// [full list](https://www.uniprot.org/help/return_fields); use `Custom` for the rest.
///
/// `Ft*` variants are sequence features (annotated residue ranges); `Cc*` variants are comments
/// (free-text and structured annotation blocks); `Xref*` variants are cross-references.
#[derive(Clone, PartialEq)]
pub enum Field {
    // Names and taxonomy
    Accession,
    /// The entry name, e.g. "HBA_HUMAN".
    Id,
    ProteinName,
    GeneNames,
    OrganismName,
    OrganismId,
    Lineage,
    // Sequence
    Sequence,
    Length,
    Mass,
    SequenceVersion,
    CcAlternativeProducts,
    // Function
    Ec,
    CcFunction,
    CcCatalyticActivity,
    CcCofactor,
    CcActivityRegulation,
    CcPathway,
    Kinetics,
    FtBinding,
    FtActSite,
    Rhea,
    // Misc
    Reviewed,
    ProteinExistence,
    AnnotationScore,
    Keyword,
    // Interaction
    CcSubunit,
    CcInteraction,
    // Subcellular location
    CcSubcellularLocation,
    FtTransmem,
    // Pathology
    CcDisease,
    // PTM / processing
    FtChain,
    FtSignal,
    FtDisulfid,
    FtModRes,
    FtCarbohyd,
    // Structure
    Structure3d,
    FtHelix,
    FtStrand,
    FtTurn,
    // Family and domains
    FtDomain,
    FtRegion,
    FtMotif,
    ProteinFamilies,
    CcSimilarity,
    // Gene ontology
    Go,
    // Cross references
    XrefPdb,
    XrefAlphaFoldDb,
    XrefChebi,
    XrefDrugBank,
    XrefChembl,
    XrefEnsembl,
    XrefRefSeq,
    XrefEmbl,
    XrefKegg,
    XrefReactome,
    // Dates
    DateModified,
    Version,
    /// Any return field not enumerated above, passed through verbatim.
    Custom(String),
}

impl Display for Field {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Accession => "accession",
            Self::Id => "id",
            Self::ProteinName => "protein_name",
            Self::GeneNames => "gene_names",
            Self::OrganismName => "organism_name",
            Self::OrganismId => "organism_id",
            Self::Lineage => "lineage",
            Self::Sequence => "sequence",
            Self::Length => "length",
            Self::Mass => "mass",
            Self::SequenceVersion => "sequence_version",
            Self::CcAlternativeProducts => "cc_alternative_products",
            Self::Ec => "ec",
            Self::CcFunction => "cc_function",
            Self::CcCatalyticActivity => "cc_catalytic_activity",
            Self::CcCofactor => "cc_cofactor",
            Self::CcActivityRegulation => "cc_activity_regulation",
            Self::CcPathway => "cc_pathway",
            Self::Kinetics => "kinetics",
            Self::FtBinding => "ft_binding",
            Self::FtActSite => "ft_act_site",
            Self::Rhea => "rhea",
            Self::Reviewed => "reviewed",
            Self::ProteinExistence => "protein_existence",
            Self::AnnotationScore => "annotation_score",
            Self::Keyword => "keyword",
            Self::CcSubunit => "cc_subunit",
            Self::CcInteraction => "cc_interaction",
            Self::CcSubcellularLocation => "cc_subcellular_location",
            Self::FtTransmem => "ft_transmem",
            Self::CcDisease => "cc_disease",
            Self::FtChain => "ft_chain",
            Self::FtSignal => "ft_signal",
            Self::FtDisulfid => "ft_disulfid",
            Self::FtModRes => "ft_mod_res",
            Self::FtCarbohyd => "ft_carbohyd",
            Self::Structure3d => "structure_3d",
            Self::FtHelix => "ft_helix",
            Self::FtStrand => "ft_strand",
            Self::FtTurn => "ft_turn",
            Self::FtDomain => "ft_domain",
            Self::FtRegion => "ft_region",
            Self::FtMotif => "ft_motif",
            Self::ProteinFamilies => "protein_families",
            Self::CcSimilarity => "cc_similarity",
            Self::Go => "go",
            Self::XrefPdb => "xref_pdb",
            Self::XrefAlphaFoldDb => "xref_alphafolddb",
            Self::XrefChebi => "xref_chebi",
            Self::XrefDrugBank => "xref_drugbank",
            Self::XrefChembl => "xref_chembl",
            Self::XrefEnsembl => "xref_ensembl",
            Self::XrefRefSeq => "xref_refseq",
            Self::XrefEmbl => "xref_embl",
            Self::XrefKegg => "xref_kegg",
            Self::XrefReactome => "xref_reactome",
            Self::DateModified => "date_modified",
            Self::Version => "version",
            Self::Custom(v) => v.as_str(),
        };
        write!(f, "{v}")
    }
}

/// The response formats UniProtKB's endpoints serve.
/// https://www.uniprot.org/help/api_queries
#[derive(Clone, Copy, Default, PartialEq)]
pub enum Format {
    #[default]
    Json,
    Tsv,
    Fasta,
    /// The flat-file format, as served by the legacy website.
    Txt,
    Xml,
    /// Accessions only, one per line.
    List,
    Gff,
}

impl Display for Format {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::Json => "json",
            Self::Tsv => "tsv",
            Self::Fasta => "fasta",
            Self::Txt => "txt",
            Self::Xml => "xml",
            Self::List => "list",
            Self::Gff => "gff",
        };
        write!(f, "{v}")
    }
}

/// Databases the ID-mapping service can translate between. This is a curated subset of the
/// [full list](https://rest.uniprot.org/configure/idmapping/fields); use `Custom` for the rest.
///
/// One side of a mapping must be UniProt: use `UniProtKbAcId` as the source, or `UniProtKb` as the
/// target. Note that these two aren't interchangeable — the source form is an accession or entry
/// name, while the target form is a full record.
#[derive(Clone, PartialEq)]
pub enum Database {
    /// Source only: a UniProtKB accession or entry name.
    UniProtKbAcId,
    /// Target only.
    UniProtKb,
    /// Target only; reviewed (Swiss-Prot) entries.
    UniProtKbSwissProt,
    UniParc,
    UniRef50,
    UniRef90,
    UniRef100,
    GeneName,
    Pdb,
    RefSeqProtein,
    RefSeqNucleotide,
    EmblGenBankDdbj,
    EmblGenBankDdbjCds,
    Ensembl,
    GeneId,
    Kegg,
    ChemBl,
    DrugBank,
    String_,
    BioGrid,
    Reactome,
    GeneCards,
    Hgnc,
    Mim,
    /// Any database not enumerated above, passed through verbatim.
    Custom(String),
}

impl Display for Database {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        let v = match self {
            Self::UniProtKbAcId => "UniProtKB_AC-ID",
            Self::UniProtKb => "UniProtKB",
            Self::UniProtKbSwissProt => "UniProtKB-Swiss-Prot",
            Self::UniParc => "UniParc",
            Self::UniRef50 => "UniRef50",
            Self::UniRef90 => "UniRef90",
            Self::UniRef100 => "UniRef100",
            Self::GeneName => "Gene_Name",
            Self::Pdb => "PDB",
            Self::RefSeqProtein => "RefSeq_Protein",
            Self::RefSeqNucleotide => "RefSeq_Nucleotide",
            Self::EmblGenBankDdbj => "EMBL-GenBank-DDBJ",
            Self::EmblGenBankDdbjCds => "EMBL-GenBank-DDBJ_CDS",
            Self::Ensembl => "Ensembl",
            Self::GeneId => "GeneID",
            Self::Kegg => "KEGG",
            Self::ChemBl => "ChEMBL",
            Self::DrugBank => "DrugBank",
            Self::String_ => "STRING",
            Self::BioGrid => "BioGRID",
            Self::Reactome => "Reactome",
            Self::GeneCards => "GeneCards",
            Self::Hgnc => "HGNC",
            Self::Mim => "MIM",
            Self::Custom(v) => v.as_str(),
        };
        write!(f, "{v}")
    }
}

// ---- Record types ---------------------------------------------------------
//
// Most fields here are optional or defaulted: a response only carries the fields that were asked
// for, so a `Field`-limited request would otherwise fail to deserialize.

/// The provenance of an annotation, e.g. an experiment reported in a specific paper.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Evidence {
    /// An [ECO](https://www.evidenceontology.org) code, e.g. "ECO:0000269" for experimental.
    pub evidence_code: String,
    /// E.g. "PubMed", "PROSITE-ProRule".
    pub source: Option<String>,
    /// The identifier within `source`, e.g. a PubMed id.
    pub id: Option<String>,
}

/// A string with the evidence backing it. UniProt uses this shape for names, free text, and most
/// other annotated values.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EvidencedString {
    pub value: String,
    pub evidences: Vec<Evidence>,
}

/// A reference to a record in another database, e.g. `ChEBI:CHEBI:15379`.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct DbReference {
    /// E.g. "Rhea", "ChEBI", "PubMed", "MIM".
    pub database: String,
    /// Generally prefixed, e.g. "RHEA:10736".
    pub id: String,
}

impl DbReference {
    /// The identifier with its database prefix removed, e.g. "RHEA:10736" -> "10736".
    pub fn id_bare(&self) -> &str {
        match self.id.split_once(':') {
            Some((_, v)) => v,
            None => &self.id,
        }
    }
}

/// One of the names a protein goes by.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProteinName {
    pub full_name: EvidencedString,
    pub short_names: Vec<EvidencedString>,
    pub ec_numbers: Vec<EvidencedString>,
}

/// The naming section of an entry. `recommended_name` is present for curated entries;
/// `submission_names` takes its place in most TrEMBL ones.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ProteinDescription {
    pub recommended_name: Option<ProteinName>,
    pub submission_names: Vec<ProteinName>,
    pub alternative_names: Vec<ProteinName>,
    /// Chains and peptides this entry is cleaved into, each with its own naming section.
    pub contains: Vec<ProteinDescription>,
    /// Domains this entry includes, each with its own naming section.
    pub includes: Vec<ProteinDescription>,
}

impl ProteinDescription {
    /// The recommended name, falling back to the first submitted, then alternative, name.
    pub fn name(&self) -> Option<&str> {
        self.recommended_name
            .as_ref()
            .or_else(|| self.submission_names.first())
            .or_else(|| self.alternative_names.first())
            .map(|n| n.full_name.value.as_str())
    }
}

/// The gene, or genes, coding for this protein.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Gene {
    pub gene_name: Option<EvidencedString>,
    pub synonyms: Vec<EvidencedString>,
    pub ordered_locus_names: Vec<EvidencedString>,
    pub orf_names: Vec<EvidencedString>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Organism {
    pub scientific_name: String,
    pub common_name: Option<String>,
    /// The NCBI taxonomy identifier, e.g. 9606 for human.
    pub taxon_id: u32,
    /// From domain down to genus.
    pub lineage: Vec<String>,
}

/// The canonical sequence of an entry. See `load_fasta` for isoform sequences.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Sequence {
    /// One-letter amino acid codes. See `Protein::seq_aa` to parse this.
    pub value: String,
    pub length: u32,
    /// Average mass, in Da.
    pub mol_weight: u32,
    pub crc64: Option<String>,
    pub md5: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct EntryAudit {
    pub first_public_date: Option<String>,
    pub last_annotation_update_date: Option<String>,
    pub last_sequence_update_date: Option<String>,
    pub entry_version: Option<u32>,
    pub sequence_version: Option<u32>,
}

/// A controlled-vocabulary term, e.g. "3D-structure" or "Oxygen transport".
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Keyword {
    /// E.g. "KW-0002".
    pub id: String,
    /// E.g. "Technical term", "PTM", "Molecular function".
    pub category: Option<String>,
    pub name: String,
}

/// One key-value pair carried by a cross reference, e.g. a PDB entry's resolution.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Property {
    pub key: String,
    pub value: String,
}

/// A link from this entry into another database. What `properties` holds varies by database.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct CrossReference {
    /// E.g. "PDB", "AlphaFoldDB", "DrugBank", "EMBL".
    pub database: String,
    pub id: String,
    pub properties: Vec<Property>,
    /// Set when this reference applies to one isoform only, e.g. "P04637-2".
    pub isoform_id: Option<String>,
}

impl CrossReference {
    /// The value of a property, by key. Keys are database-specific; a PDB reference, for example,
    /// carries "Method", "Resolution" and "Chains".
    pub fn property(&self, key: &str) -> Option<&str> {
        self.properties
            .iter()
            .find(|p| p.key == key)
            .map(|p| p.value.as_str())
    }
}

/// A PDB structure of this protein, from the entry's cross references. Pass `id` to the `rcsb` or
/// `pdbe` modules to load the structure itself.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct PdbXref {
    /// E.g. "1A00".
    pub id: String,
    /// E.g. "X-ray", "NMR", "EM".
    pub method: Option<String>,
    /// In Å. Absent for methods that don't report one, e.g. NMR.
    pub resolution: Option<f32>,
    /// The chains of the structure this entry covers, and over which residues, e.g. "A/C=2-142".
    pub chains: Option<String>,
}

/// A residue position within a feature. `value` is absent when the position is unknown.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Position {
    /// 1-based, into the canonical sequence.
    pub value: Option<u32>,
    /// E.g. "EXACT", "OUTSIDE", "UNKNOWN".
    pub modifier: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct FeatureLocation {
    pub start: Position,
    pub end: Position,
}

/// A small molecule bound by a binding-site feature.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Ligand {
    /// E.g. "O2".
    pub name: Option<String>,
    /// Doubly prefixed, e.g. "ChEBI:CHEBI:15379".
    pub id: Option<String>,
    /// Which copy of the ligand this site binds, when an entry has several.
    pub label: Option<String>,
}

impl Ligand {
    /// The ChEBI id of this ligand, e.g. 15379. Pass this to `chebi::load_compound`.
    pub fn chebi_id(&self) -> Option<u32> {
        parse_chebi_id(self.id.as_deref()?)
    }
}

/// An annotated range of the sequence, e.g. a domain, a binding site, or a helix.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Feature {
    /// E.g. "Chain", "Domain", "Binding site", "Helix", "Natural variant".
    #[serde(rename = "type")]
    pub type_: String,
    pub location: FeatureLocation,
    pub description: Option<String>,
    /// E.g. "PRO_0000052653" for a chain, "VAR_002715" for a variant.
    pub feature_id: Option<String>,
    /// Present on binding sites.
    pub ligand: Option<Ligand>,
    pub feature_cross_references: Vec<DbReference>,
    pub evidences: Vec<Evidence>,
}

impl Feature {
    /// The 1-based, inclusive residue range this feature covers, if both ends are known.
    pub fn range(&self) -> Option<(u32, u32)> {
        Some((self.location.start.value?, self.location.end.value?))
    }
}

/// A reaction this protein catalyses, from a "CATALYTIC ACTIVITY" comment.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Reaction {
    /// E.g. "a primary alcohol + NAD(+) = an aldehyde + NADH + H(+)".
    pub name: Option<String>,
    /// The Rhea reaction, and its ChEBI participants.
    pub reaction_cross_references: Vec<DbReference>,
    /// E.g. "1.1.1.1".
    pub ec_number: Option<String>,
}

impl Reaction {
    /// The numeric portion of this reaction's Rhea master id, e.g. 10736. Pass this to
    /// `rhea::load_reaction`.
    pub fn rhea_id(&self) -> Option<u32> {
        self.reaction_cross_references
            .iter()
            .find(|x| x.database == "Rhea")
            .and_then(|x| x.id_bare().parse().ok())
    }

    /// The ChEBI ids of the participants, on both sides of the equation.
    pub fn chebi_ids(&self) -> Vec<u32> {
        self.reaction_cross_references
            .iter()
            .filter(|x| x.database == "ChEBI")
            .filter_map(|x| x.id_bare().parse().ok())
            .collect()
    }
}

/// A cofactor this protein requires, e.g. a metal ion.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Cofactor {
    /// E.g. "Zn(2+)".
    pub name: Option<String>,
    pub cofactor_cross_reference: Option<DbReference>,
}

impl Cofactor {
    /// The ChEBI id of this cofactor, e.g. 29105.
    pub fn chebi_id(&self) -> Option<u32> {
        parse_chebi_id(&self.cofactor_cross_reference.as_ref()?.id)
    }
}

/// Where in the cell this protein is found.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct SubcellularLocation {
    /// E.g. "Cytoplasm".
    pub location: Option<EvidencedString>,
    /// E.g. "Single-pass membrane protein".
    pub topology: Option<EvidencedString>,
}

/// A disease this protein is implicated in.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Disease {
    /// E.g. "Alpha-thalassemia".
    pub disease_id: Option<String>,
    /// UniProt's own accession for the disease, e.g. "DI-01181".
    pub disease_accession: Option<String>,
    /// E.g. "A-THAL".
    pub acronym: Option<String>,
    pub description: Option<String>,
    /// Generally into OMIM (`MIM`).
    pub disease_cross_reference: Option<DbReference>,
}

/// A splice, or alternative promoter, variant of this protein.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Isoform {
    pub name: Option<EvidencedString>,
    pub synonyms: Vec<EvidencedString>,
    /// E.g. "P04637-2". Pass this to `load_fasta` for the isoform's sequence.
    pub isoform_ids: Vec<String>,
    /// "Displayed" for the canonical sequence; otherwise "Described", "External" or "Not described".
    pub isoform_sequence_status: Option<String>,
}

/// One side of a binary protein-protein interaction.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Interactant {
    #[serde(rename = "uniProtKBAccession")]
    pub accession: Option<String>,
    pub gene_name: Option<String>,
    /// The IntAct identifier, e.g. "EBI-714680".
    pub int_act_id: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Interaction {
    pub interactant_one: Interactant,
    pub interactant_two: Interactant,
    pub number_of_experiments: Option<u32>,
    /// Whether the two partners come from different organisms.
    pub organism_differ: bool,
}

/// A measured Michaelis constant for one substrate.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MichaelisConstant {
    pub constant: f32,
    /// E.g. "uM", "mM".
    pub unit: String,
    pub substrate: String,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct MaximumVelocity {
    pub velocity: f32,
    pub unit: String,
    pub enzyme: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct KineticParameters {
    pub michaelis_constants: Vec<MichaelisConstant>,
    pub maximum_velocities: Vec<MaximumVelocity>,
}

/// Free-text with its evidence, as carried by a comment's `note`.
///
/// Most comment types nest this under `texts`; a few, e.g. "WEB RESOURCE", carry a bare string
/// instead. We normalize the two into this shape.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(from = "NoteRaw")]
pub struct Note {
    pub texts: Vec<EvidencedString>,
}

impl Note {
    /// The first free-text value of this note.
    pub fn text(&self) -> Option<&str> {
        self.texts.first().map(|t| t.value.as_str())
    }
}

/// Deserializing only; see `Note`.
#[derive(Deserialize)]
#[serde(untagged)]
enum NoteRaw {
    Text(String),
    Structured {
        #[serde(default)]
        texts: Vec<EvidencedString>,
    },
}

impl From<NoteRaw> for Note {
    fn from(v: NoteRaw) -> Self {
        let texts = match v {
            NoteRaw::Text(value) => vec![EvidencedString {
                value,
                evidences: Vec::new(),
            }],
            NoteRaw::Structured { texts } => texts,
        };

        Self { texts }
    }
}

/// An annotation block. UniProt has roughly two dozen comment types, each with its own payload, so
/// this is a union: only the fields relevant to `comment_type` are populated.
///
/// See `Protein::comments_of_type` and the accessors alongside it.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Comment {
    /// E.g. "FUNCTION", "CATALYTIC ACTIVITY", "SUBCELLULAR LOCATION", "DISEASE", "SUBUNIT".
    pub comment_type: String,
    /// Which chain, peptide or isoform this comment applies to, when not the whole entry.
    pub molecule: Option<String>,
    /// The payload of the free-text comment types, e.g. "FUNCTION" and "SUBUNIT".
    pub texts: Vec<EvidencedString>,
    pub note: Option<Note>,
    /// "CATALYTIC ACTIVITY".
    pub reaction: Option<Reaction>,
    /// "COFACTOR".
    pub cofactors: Vec<Cofactor>,
    /// "SUBCELLULAR LOCATION".
    pub subcellular_locations: Vec<SubcellularLocation>,
    /// "DISEASE".
    pub disease: Option<Disease>,
    /// "ALTERNATIVE PRODUCTS".
    pub isoforms: Vec<Isoform>,
    /// "ALTERNATIVE PRODUCTS"; e.g. "Alternative splicing".
    pub events: Vec<String>,
    /// "INTERACTION".
    pub interactions: Vec<Interaction>,
    /// "BIOPHYSICOCHEMICAL PROPERTIES".
    pub kinetic_parameters: Option<KineticParameters>,
    /// "WEB RESOURCE".
    pub resource_name: Option<String>,
    /// "WEB RESOURCE".
    pub resource_url: Option<String>,
}

impl Comment {
    /// The first free-text value of this comment.
    pub fn text(&self) -> Option<&str> {
        self.texts.first().map(|t| t.value.as_str())
    }
}

/// A publication supporting this entry's annotations.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Citation {
    pub id: Option<String>,
    /// E.g. "journal article", "submission", "book".
    pub citation_type: Option<String>,
    pub authors: Vec<String>,
    pub title: Option<String>,
    pub journal: Option<String>,
    /// Generally the year alone, e.g. "1980".
    pub publication_date: Option<String>,
    pub volume: Option<String>,
    pub first_page: Option<String>,
    pub last_page: Option<String>,
    pub citation_cross_references: Vec<DbReference>,
}

impl Citation {
    pub fn pubmed_id(&self) -> Option<u32> {
        self.citation_cross_references
            .iter()
            .find(|x| x.database == "PubMed")
            .and_then(|x| x.id.parse().ok())
    }

    pub fn doi(&self) -> Option<&str> {
        self.citation_cross_references
            .iter()
            .find(|x| x.database == "DOI")
            .map(|x| x.id.as_str())
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Reference {
    pub reference_number: Option<u32>,
    pub citation: Option<Citation>,
    /// What this paper contributed, e.g. "NUCLEOTIDE SEQUENCE [GENOMIC DNA]".
    pub reference_positions: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct ExtraAttributes {
    /// The UniParc identifier for this sequence, e.g. "UPI0000000239".
    pub uni_parc_id: Option<String>,
}

/// A full UniProtKB entry.
///
/// Records fetched with a `Field` list carry only the fields requested; everything here beyond
/// `entry_type` and `primary_accession` may therefore be empty.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Protein {
    /// "UniProtKB reviewed (Swiss-Prot)" or "UniProtKB unreviewed (TrEMBL)". See `reviewed`.
    pub entry_type: String,
    /// E.g. "P69905".
    pub primary_accession: String,
    /// Accessions that were merged into this entry, and still resolve to it.
    pub secondary_accessions: Vec<String>,
    /// The entry name, e.g. "HBA_HUMAN". Unlike the accession, this is not stable across releases.
    #[serde(rename = "uniProtkbId")]
    pub entry_name: String,
    pub entry_audit: Option<EntryAudit>,
    /// UniProt's 1-5 rating of how much annotation an entry carries.
    pub annotation_score: f32,
    pub organism: Option<Organism>,
    /// E.g. "Evidence at protein level", "Inferred from homology".
    pub protein_existence: Option<String>,
    pub protein_description: ProteinDescription,
    pub genes: Vec<Gene>,
    pub comments: Vec<Comment>,
    pub features: Vec<Feature>,
    pub keywords: Vec<Keyword>,
    pub references: Vec<Reference>,
    #[serde(rename = "uniProtKBCrossReferences")]
    pub cross_references: Vec<CrossReference>,
    pub sequence: Option<Sequence>,
    pub extra_attributes: Option<ExtraAttributes>,
}

impl Protein {
    /// Whether this entry is manually curated, i.e. from Swiss-Prot rather than TrEMBL.
    pub fn reviewed(&self) -> bool {
        self.entry_type.contains("Swiss-Prot")
    }

    /// The recommended name, e.g. "Hemoglobin subunit alpha".
    pub fn name(&self) -> String {
        self.protein_description
            .name()
            .unwrap_or_default()
            .to_owned()
    }

    /// The primary name of each gene coding for this protein, e.g. ["HBA1", "HBA2"].
    pub fn gene_names(&self) -> Vec<String> {
        self.genes
            .iter()
            .filter_map(|g| g.gene_name.as_ref())
            .map(|n| n.value.clone())
            .collect()
    }

    /// EC numbers, e.g. ["1.1.1.1"]. These come from the entry's names, and from the reactions it
    /// catalyses; we merge the two.
    pub fn ec_numbers(&self) -> Vec<String> {
        let mut result: Vec<String> = self
            .protein_description
            .recommended_name
            .iter()
            .chain(&self.protein_description.submission_names)
            .flat_map(|n| n.ec_numbers.iter())
            .map(|e| e.value.clone())
            .collect();

        for r in self.catalytic_activities() {
            if let Some(ec) = &r.ec_number
                && !result.contains(ec)
            {
                result.push(ec.clone());
            }
        }

        result
    }

    /// The canonical sequence, as amino acids. Non-standard residues UniProt represents with
    /// `B`, `J`, `X` or `Z` have no single amino acid, and are dropped; compare the result's
    /// length against `Sequence::length` if that matters.
    pub fn seq_aa(&self) -> Vec<AminoAcid> {
        match &self.sequence {
            Some(s) => seq_aa_from_str(&s.value),
            None => Vec::new(),
        }
    }

    pub fn comments_of_type(&self, type_: &str) -> Vec<&Comment> {
        self.comments
            .iter()
            .filter(|c| c.comment_type == type_)
            .collect()
    }

    /// The first free-text value of a comment type, e.g. "SUBUNIT" or "TISSUE SPECIFICITY".
    pub fn text_of_type(&self, type_: &str) -> Option<String> {
        self.comments_of_type(type_)
            .first()
            .and_then(|c| c.text())
            .map(str::to_owned)
    }

    /// What this protein does, in prose.
    pub fn function(&self) -> Option<String> {
        self.text_of_type("FUNCTION")
    }

    /// The reactions this protein catalyses.
    pub fn catalytic_activities(&self) -> Vec<&Reaction> {
        self.comments
            .iter()
            .filter_map(|c| c.reaction.as_ref())
            .collect()
    }

    /// The Rhea master ids of the reactions this protein catalyses. Pass these to
    /// `rhea::load_reaction`.
    pub fn rhea_ids(&self) -> Vec<u32> {
        self.catalytic_activities()
            .iter()
            .filter_map(|r| r.rhea_id())
            .collect()
    }

    /// Every ChEBI entity this entry references: bound ligands, cofactors, and the participants of
    /// the reactions it catalyses. Pass these to `chebi::load_compound`.
    pub fn chebi_ids(&self) -> Vec<u32> {
        let ligands = self.features.iter().filter_map(|f| f.ligand.as_ref());
        let cofactors = self.comments.iter().flat_map(|c| c.cofactors.iter());

        let mut result: Vec<u32> = Vec::new();
        for id in ligands
            .filter_map(Ligand::chebi_id)
            .chain(cofactors.filter_map(Cofactor::chebi_id))
            .chain(
                self.catalytic_activities()
                    .iter()
                    .flat_map(|r| r.chebi_ids()),
            )
        {
            if !result.contains(&id) {
                result.push(id);
            }
        }

        result
    }

    /// Where in the cell this protein is found, e.g. ["Cytoplasm"].
    pub fn subcellular_locations(&self) -> Vec<String> {
        self.comments
            .iter()
            .flat_map(|c| c.subcellular_locations.iter())
            .filter_map(|l| l.location.as_ref())
            .map(|l| l.value.clone())
            .collect()
    }

    pub fn diseases(&self) -> Vec<&Disease> {
        self.comments
            .iter()
            .filter_map(|c| c.disease.as_ref())
            .collect()
    }

    /// The isoform accessions of this entry, e.g. ["P04637-1", "P04637-2"]. Pass these to
    /// `load_fasta` for their sequences.
    pub fn isoform_ids(&self) -> Vec<String> {
        self.comments
            .iter()
            .flat_map(|c| c.isoforms.iter())
            .flat_map(|i| i.isoform_ids.iter())
            .cloned()
            .collect()
    }

    /// Sequence features of a given type, e.g. "Binding site", "Domain", "Helix", "Chain".
    pub fn features_of_type(&self, type_: &str) -> Vec<&Feature> {
        self.features.iter().filter(|f| f.type_ == type_).collect()
    }

    /// Cross references into a given database, e.g. "PDB", "DrugBank", "AlphaFoldDB".
    pub fn xrefs(&self, database: &str) -> Vec<&CrossReference> {
        self.cross_references
            .iter()
            .filter(|x| x.database == database)
            .collect()
    }

    /// The identifiers of the cross references into a given database.
    pub fn xref_ids(&self, database: &str) -> Vec<String> {
        self.xrefs(database).iter().map(|x| x.id.clone()).collect()
    }

    /// The PDB entries containing this protein, e.g. ["1A00", "4HHB"]. Pass these to
    /// `rcsb::load_cif`. Note that well-studied proteins have hundreds.
    pub fn pdb_ids(&self) -> Vec<String> {
        self.xref_ids("PDB")
    }

    /// As `pdb_ids`, with each structure's method, resolution and chain coverage.
    pub fn pdb_xrefs(&self) -> Vec<PdbXref> {
        self.xrefs("PDB")
            .into_iter()
            .map(|x| PdbXref {
                id: x.id.clone(),
                method: x.property("Method").map(str::to_owned),
                // E.g. "2.00 A"; the unit is always Å.
                resolution: x
                    .property("Resolution")
                    .and_then(|v| v.trim_end_matches(" A").parse().ok()),
                chains: x.property("Chains").map(str::to_owned),
            })
            .collect()
    }
}

/// A curated subset of a UniProtKB record, for applications that don't need the full entry.
/// Analogous to `pubchem::Properties` and `chebi::Properties`.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct Properties {
    /// E.g. "P69905".
    pub accession: String,
    /// The entry name, e.g. "HBA_HUMAN".
    pub entry_name: String,
    /// The recommended name, e.g. "Hemoglobin subunit alpha".
    pub name: String,
    /// Whether this entry is manually curated (Swiss-Prot) rather than automatic (TrEMBL).
    pub reviewed: bool,
    pub gene_names: Vec<String>,
    /// E.g. "Homo sapiens".
    pub organism: String,
    /// The NCBI taxonomy identifier, e.g. 9606.
    pub taxon_id: u32,
    /// Residue count of the canonical sequence.
    pub length: u32,
    /// Average mass, in Da.
    pub mol_weight: u32,
    /// E.g. ["1.1.1.1"]. Empty for non-enzymes.
    pub ec_numbers: Vec<String>,
    /// What this protein does, in prose.
    pub function: Option<String>,
    /// One-letter amino acid codes.
    pub sequence: String,
    /// PDB entries containing this protein. Pass these to `rcsb::load_cif`.
    pub pdb_ids: Vec<String>,
}

impl From<&Protein> for Properties {
    fn from(p: &Protein) -> Self {
        let (organism, taxon_id) = match &p.organism {
            Some(o) => (o.scientific_name.clone(), o.taxon_id),
            None => (String::new(), 0),
        };

        let (sequence, length, mol_weight) = match &p.sequence {
            Some(s) => (s.value.clone(), s.length, s.mol_weight),
            None => (String::new(), 0, 0),
        };

        Self {
            accession: p.primary_accession.clone(),
            entry_name: p.entry_name.clone(),
            name: p.name(),
            reviewed: p.reviewed(),
            gene_names: p.gene_names(),
            organism,
            taxon_id,
            length,
            mol_weight,
            ec_numbers: p.ec_numbers(),
            function: p.function(),
            sequence,
            pdb_ids: p.pdb_ids(),
        }
    }
}

/// The fields `Properties` is built from. A full record is a much larger download.
const PROPERTIES_FIELDS: [Field; 12] = [
    Field::Accession,
    Field::Id,
    Field::ProteinName,
    Field::GeneNames,
    Field::OrganismName,
    Field::OrganismId,
    Field::Length,
    Field::Mass,
    Field::Ec,
    Field::CcFunction,
    Field::Sequence,
    Field::XrefPdb,
];

/// One row of an ID-mapping result. Identifiers that map to several targets appear once per target.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct IdMapping {
    /// The identifier as submitted, e.g. "1A00".
    pub from: String,
    /// The identifier it maps to, e.g. "P69905".
    pub to: String,
}

/// A predicted structure from AlphaFold DB. Proteins too long to model in one piece are split into
/// overlapping fragments, one prediction each.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct AlphaFoldPrediction {
    /// E.g. "AF-P69905-F1".
    pub entry_id: String,
    /// E.g. "P69905".
    pub uniprot_accession: Option<String>,
    /// Mean pLDDT over the model, from 0-100. Above 90 is high confidence; below 50 generally
    /// indicates a disordered region rather than a fold.
    pub global_metric_value: Option<f32>,
    /// The AlphaFold DB release this model is from.
    pub latest_version: Option<u32>,
    pub model_created_date: Option<String>,
    /// The residue range of the UniProt sequence this model covers, 1-based.
    pub uniprot_start: Option<u32>,
    pub uniprot_end: Option<u32>,
    /// The modelled sequence, as one-letter codes.
    pub uniprot_sequence: Option<String>,
    pub cif_url: Option<String>,
    pub pdb_url: Option<String>,
    /// Binary CIF; smaller than `cif_url`, at the cost of needing a parser for it.
    pub bcif_url: Option<String>,
    /// Per-residue pLDDT confidence scores, as JSON.
    pub plddt_doc_url: Option<String>,
    /// The predicted aligned error matrix, as JSON.
    pub pae_doc_url: Option<String>,
}

// ---- Requests -------------------------------------------------------------

/// Accepts `P69905`, `uniprot:P69905` or `UniProtKB:P69905`; returns the bare accession. Isoform
/// suffixes, e.g. `P69905-1`, are preserved.
pub fn parse_accession(ident: &str) -> String {
    let v = ident.trim();

    match v.split_once(':') {
        Some((prefix, acc))
            if prefix.eq_ignore_ascii_case("uniprot")
                || prefix.eq_ignore_ascii_case("uniprotkb") =>
        {
            acc.trim().to_uppercase()
        }
        _ => v.to_uppercase(),
    }
}

/// Strip both prefixes from a ChEBI reference: UniProt writes ligand ids as `ChEBI:CHEBI:15379`,
/// and cofactor ones as `CHEBI:29105`.
fn parse_chebi_id(id: &str) -> Option<u32> {
    id.rsplit(':').next()?.parse().ok()
}

/// UniProt asks that programs identify themselves, so we set a User-Agent. We also ask for an
/// unencoded body: UniProt gzips its responses when offered the chance, and our agent hands those
/// back compressed.
fn request(url: &str) -> Result<ureq::http::Response<ureq::Body>, ReqError> {
    let agent = make_agent();

    Ok(agent
        .get(url)
        .header("User-Agent", USER_AGENT)
        .header("Accept-Encoding", "identity")
        .call()?)
}

/// Our agent doesn't treat error status codes as errors, and UniProt returns a JSON body on
/// failure, e.g. `{"messages": ["The 'accession' value has invalid format"]}`. Catch that here, so
/// we don't hand a failure message back to the caller as if it were data.
fn get(url: &str) -> Result<String, ReqError> {
    let mut resp = request(url)?;

    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    Ok(resp.body_mut().read_to_string()?)
}

/// The URL of the next page, from UniProt's `Link` header: `<url>; rel="next"`.
fn parse_next_link(header: &str) -> Option<String> {
    if !header.contains("rel=\"next\"") {
        return None;
    }

    let start = header.find('<')? + 1;
    let end = header.find('>')?;

    Some(header[start..end].to_owned())
}

/// Walk UniProt's cursor pagination, returning each page's body. `max_pages` of `None` follows
/// every `next` link, which for a broad query is a great many requests.
fn get_pages(url: &str, max_pages: Option<u32>) -> Result<Vec<String>, ReqError> {
    let mut next = Some(url.to_owned());
    let mut result = Vec::new();

    while let Some(u) = next {
        let mut resp = request(&u)?;

        if resp.status() != 200 {
            return Err(ReqError::Http);
        }

        next = resp
            .headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_next_link);

        result.push(resp.body_mut().read_to_string()?);

        if let Some(m) = max_pages
            && result.len() >= m as usize
        {
            break;
        }
    }

    Ok(result)
}

/// The `fields` query parameter, as a comma-separated list. An empty list means every field.
fn fields_param(fields: &[Field]) -> Option<String> {
    if fields.is_empty() {
        return None;
    }

    let v: Vec<String> = fields.iter().map(|f| f.to_string()).collect();
    Some(v.join(","))
}

/// How many pages of `page_size` we need to satisfy `limit`.
fn max_pages(limit: Option<u32>, page_size: u32) -> Option<u32> {
    limit.map(|l| l.div_ceil(page_size))
}

pub fn open_overview(accession: &str) {
    let url = format!("{BASE_URL}/{}/entry", parse_accession(accession));

    if let Err(e) = webbrowser::open(&url) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Open the browser to a protein's predicted structure in AlphaFold DB.
pub fn open_alphafold_view(accession: &str) {
    let url = format!("{ALPHAFOLD_URL}/entry/{}", parse_accession(accession));

    if let Err(e) = webbrowser::open(&url) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Calls the [search API](https://www.uniprot.org/help/api_queries) directly, returning the raw
/// body in the format requested.
///
/// The query uses the same syntax as the website's search box, e.g. `insulin`,
/// `gene:HBA1 AND organism_id:9606`, `accession:P69905`, `xref:pdb-1A00`, `ec:1.1.1.1`,
/// `taxonomy_id:9606 AND reviewed:true`, or `(cc_catalytic_activity:"rhea:10736")`. An empty
/// `fields` list returns every field, which for JSON is a large download.
///
/// This returns the first page only; `size` is capped at 500.
pub fn query_search(
    query: &str,
    fields: &[Field],
    format: Format,
    size: Option<u32>,
) -> Result<String, ReqError> {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("query", query);
    params.append_pair("format", &format.to_string());

    if let Some(f) = fields_param(fields) {
        params.append_pair("fields", &f);
    }

    if let Some(s) = size {
        params.append_pair("size", &s.min(PAGE_SIZE_MAX).to_string());
    }

    get(&format!("{API_URL}/search?{}", params.finish()))
}

/// Fetch a single entry directly, returning the raw body in the format requested. See
/// `query_search` regarding an empty `fields` list.
pub fn query_entry(accession: &str, fields: &[Field], format: Format) -> Result<String, ReqError> {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("format", &format.to_string());

    if let Some(f) = fields_param(fields) {
        params.append_pair("fields", &f);
    }

    let url = format!(
        "{API_URL}/{}?{}",
        parse_accession(accession),
        params.finish()
    );

    get(&url)
}

/// Load the full UniProtKB record for a protein. Secondary (merged) accessions resolve to their
/// primary entry.
///
/// This is a large download for a well-studied protein — see `load_protein_fields` and
/// `properties` for lighter alternatives.
pub fn load_protein(accession: &str) -> Result<Protein, ReqError> {
    Ok(serde_json::from_str(&query_entry(
        accession,
        &[],
        Format::Json,
    )?)?)
}

/// Load part of a record: only the fields requested are populated. Note that `entry_type` and
/// `primary_accession` come back regardless.
pub fn load_protein_fields(accession: &str, fields: &[Field]) -> Result<Protein, ReqError> {
    Ok(serde_json::from_str(&query_entry(
        accession,
        fields,
        Format::Json,
    )?)?)
}

/// Deserializing only; the search and batch endpoints wrap entries in a results envelope.
#[derive(Debug, Deserialize)]
struct SearchResp {
    results: Vec<Protein>,
}

/// Load several records in a single request. Well-formed accessions UniProt has no entry for are
/// simply absent from the result, but a *malformed* one, e.g. "NOPE99", fails the whole request
/// with `ReqError::Http`. The caller should chunk very large lists, as a long request URL can be
/// rejected.
///
/// An empty `fields` list returns full records, which is a large download for several proteins at
/// once.
pub fn load_proteins(accessions: &[String], fields: &[Field]) -> Result<Vec<Protein>, ReqError> {
    if accessions.is_empty() {
        return Ok(Vec::new());
    }

    let accs: Vec<String> = accessions.iter().map(|a| parse_accession(a)).collect();

    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("accessions", &accs.join(","));

    if let Some(f) = fields_param(fields) {
        params.append_pair("fields", &f);
    }

    let url = format!("{API_URL}/accessions?{}", params.finish());

    let parsed: SearchResp = serde_json::from_str(&get(&url)?)?;
    Ok(parsed.results)
}

/// A curated subset of a record's data. See `load_protein` for everything UniProt has.
pub fn properties(accession: &str) -> Result<Properties, ReqError> {
    Ok((&load_protein_fields(accession, &PROPERTIES_FIELDS)?).into())
}

/// Download a protein's sequence in FASTA format. This accepts isoform accessions, e.g.
/// `P04637-2`, which the JSON endpoints don't serve.
pub fn load_fasta(accession: &str) -> Result<String, ReqError> {
    get(&format!("{API_URL}/{}.fasta", parse_accession(accession)))
}

/// Load a protein's canonical sequence, as amino acids. See `Protein::seq_aa` regarding
/// non-standard residues.
pub fn load_sequence(accession: &str) -> Result<Vec<AminoAcid>, ReqError> {
    let protein = load_protein_fields(accession, &[Field::Sequence])?;
    Ok(protein.seq_aa())
}

/// Search for proteins. See `query_search` for the query syntax, and for what an empty `fields`
/// list costs. `limit` caps the number returned; `None` walks every page, which is rarely what you
/// want — check `count` first.
pub fn search(query: &str, fields: &[Field], limit: Option<u32>) -> Result<Vec<Protein>, ReqError> {
    let page_size = limit.unwrap_or(PAGE_SIZE_MAX).min(PAGE_SIZE_MAX);

    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("query", query);
    params.append_pair("format", "json");
    params.append_pair("size", &page_size.to_string());

    if let Some(f) = fields_param(fields) {
        params.append_pair("fields", &f);
    }

    let url = format!("{API_URL}/search?{}", params.finish());

    let mut result = Vec::new();
    for page in get_pages(&url, max_pages(limit, page_size))? {
        let parsed: SearchResp = serde_json::from_str(&page)?;

        // Guard against a `next` link that doesn't advance.
        if parsed.results.is_empty() {
            break;
        }

        result.extend(parsed.results);
    }

    if let Some(l) = limit {
        result.truncate(l as usize);
    }

    Ok(result)
}

/// How many entries a query matches, without downloading them. See `query_search` for the syntax.
pub fn count(query: &str) -> Result<u32, ReqError> {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("query", query);
    params.append_pair("format", "list");
    params.append_pair("size", "1");

    let resp = request(&format!("{API_URL}/search?{}", params.finish()))?;

    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    // UniProt reports the total in a header rather than the body.
    resp.headers()
        .get("x-total-results")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse().ok())
        .ok_or(ReqError::Deserialize)
}

/// Load a list of accessions from a search. Analogous to `pubchem::find_cids_from_search`.
pub fn find_accessions_from_search(
    query: &str,
    limit: Option<u32>,
) -> Result<Vec<String>, ReqError> {
    let page_size = limit.unwrap_or(PAGE_SIZE_MAX).min(PAGE_SIZE_MAX);

    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("query", query);
    params.append_pair("format", "list");
    params.append_pair("size", &page_size.to_string());

    let url = format!("{API_URL}/search?{}", params.finish());

    let mut result = Vec::new();
    for page in get_pages(&url, max_pages(limit, page_size))? {
        let accessions: Vec<String> = page
            .lines()
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();

        // Guard against a `next` link that doesn't advance.
        if accessions.is_empty() {
            break;
        }

        result.extend(accessions);
    }

    if let Some(l) = limit {
        result.truncate(l as usize);
    }

    Ok(result)
}

/// Combine query terms, applying the reviewed (Swiss-Prot) filter that most callers want.
fn query_reviewed(query: &str, reviewed_only: bool) -> String {
    if reviewed_only {
        format!("({query}) AND (reviewed:true)")
    } else {
        query.to_owned()
    }
}

/// Find the proteins coded for by a gene, e.g. "HBA1". Passing a `taxon_id`, e.g. 9606 for human,
/// narrows this to one organism; without it, you get the gene's orthologs across all of them.
pub fn proteins_from_gene(
    gene: &str,
    taxon_id: Option<u32>,
    reviewed_only: bool,
    fields: &[Field],
    limit: Option<u32>,
) -> Result<Vec<Protein>, ReqError> {
    let mut query = format!("gene:{gene}");
    if let Some(t) = taxon_id {
        query += &format!(" AND organism_id:{t}");
    }

    search(&query_reviewed(&query, reviewed_only), fields, limit)
}

/// Find the proteins in a PDB structure, from its identifier, e.g. "1A00". This is the main bridge
/// from the `rcsb` and `pdbe` modules; see `pdbe::load_uniprot_mappings` for residue-level
/// alignments between the two.
pub fn proteins_from_pdb_id(pdb_id: &str, fields: &[Field]) -> Result<Vec<Protein>, ReqError> {
    search(
        &format!("xref:pdb-{}", pdb_id.to_uppercase()),
        fields,
        Some(PAGE_SIZE_MAX),
    )
}

/// Find the proteins annotated as catalysing a reaction, from its Rhea master id. This is the main
/// bridge from the `rhea` module; `rhea::uniprot_ids` is the lighter form, returning accessions
/// only.
///
/// Note that well-studied reactions have thousands of enzymes; check `rhea::Reaction::enzyme_count`
/// or `count` before calling this without a `limit`.
pub fn proteins_from_rhea(
    master_id: u32,
    reviewed_only: bool,
    fields: &[Field],
    limit: Option<u32>,
) -> Result<Vec<Protein>, ReqError> {
    let query = format!("(cc_catalytic_activity:\"rhea:{master_id}\")");

    search(&query_reviewed(&query, reviewed_only), fields, limit)
}

/// Deserializing only; the status endpoint answers with a job state until the job finishes, then
/// redirects to the results.
#[derive(Debug, Default, Deserialize)]
#[serde(default, rename_all = "camelCase")]
struct JobStatus {
    job_status: Option<String>,
    messages: Vec<String>,
}

/// Deserializing only. `to` is a bare identifier for most targets, but a full record when mapping
/// into UniProtKB.
#[derive(Debug, Deserialize)]
struct IdMappingRow {
    from: String,
    to: serde_json::Value,
}

/// Deserializing only.
#[derive(Debug, Deserialize)]
struct IdMappingResp {
    results: Vec<IdMappingRow>,
}

#[derive(Debug, Deserialize)]
struct JobResp {
    #[serde(rename = "jobId")]
    job_id: String,
}

/// Translate identifiers between databases, using UniProt's
/// [ID mapping service](https://www.uniprot.org/help/id_mapping). One side must be UniProt: use
/// `Database::UniProtKbAcId` as the source, or `Database::UniProtKb` as the target.
///
/// Identifiers that map to several targets appear once per target; ones with no match are simply
/// absent. `limit` caps the number of rows returned.
///
/// This is a job-based API: we submit the identifiers, poll until the job finishes, then collect
/// the results. Expect it to take a second or two, and to fail with `ReqError::Http` if the job
/// hasn't finished within ~20s.
pub fn map_ids(
    from: Database,
    to: Database,
    ids: &[String],
    limit: Option<u32>,
) -> Result<Vec<IdMapping>, ReqError> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }

    let agent = make_agent();

    let mut payload = url::form_urlencoded::Serializer::new(String::new());
    payload.append_pair("from", &from.to_string());
    payload.append_pair("to", &to.to_string());
    payload.append_pair("ids", &ids.join(","));

    let mut resp = agent
        .post(&format!("{ID_MAPPING_URL}/run"))
        .header("User-Agent", USER_AGENT)
        .header("Content-Type", "application/x-www-form-urlencoded")
        .send(&payload.finish())?;

    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    let job: JobResp = serde_json::from_str(&resp.body_mut().read_to_string()?)?;

    let status_url = format!("{ID_MAPPING_URL}/status/{}", job.job_id);
    let mut finished = false;

    for _ in 0..JOB_POLL_ATTEMPTS {
        let body = get(&status_url)?;

        // Once the job finishes, this endpoint redirects to the results, which carry no status.
        let status: JobStatus = serde_json::from_str(&body)?;

        match status.job_status.as_deref() {
            Some("NEW") | Some("RUNNING") => thread::sleep(JOB_POLL_INTERVAL),
            Some("ERROR") => return Err(ReqError::Http),
            _ => {
                if !status.messages.is_empty() {
                    return Err(ReqError::Http);
                }
                finished = true;
                break;
            }
        }
    }

    if !finished {
        return Err(ReqError::Http);
    }

    let page_size = limit.unwrap_or(PAGE_SIZE_MAX).min(PAGE_SIZE_MAX);
    let url = format!(
        "{ID_MAPPING_URL}/results/{}?format=json&size={page_size}",
        job.job_id
    );

    let mut result = Vec::new();
    for page in get_pages(&url, max_pages(limit, page_size))? {
        let parsed: IdMappingResp = serde_json::from_str(&page)?;

        if parsed.results.is_empty() {
            break;
        }

        for row in parsed.results {
            // A UniProtKB target comes back as a record rather than a bare identifier.
            let to = match row.to {
                serde_json::Value::String(v) => Some(v),
                serde_json::Value::Object(ref o) => o
                    .get("primaryAccession")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned),
                _ => None,
            };

            if let Some(to) = to {
                result.push(IdMapping { from: row.from, to });
            }
        }
    }

    if let Some(l) = limit {
        result.truncate(l as usize);
    }

    Ok(result)
}

/// Load AlphaFold DB's predicted structures for a protein. This is generally a single model;
/// proteins too long to model in one piece come back as several overlapping fragments.
///
/// Returns `ReqError::Http` for proteins AlphaFold has no model for.
pub fn load_alphafold_predictions(accession: &str) -> Result<Vec<AlphaFoldPrediction>, ReqError> {
    let url = format!(
        "{ALPHAFOLD_URL}/api/prediction/{}",
        parse_accession(accession)
    );

    Ok(serde_json::from_str(&get(&url)?)?)
}

/// The URL of a prediction's structure file. We take these from the API rather than building them,
/// as they carry the model version, which AlphaFold bumps on each release.
fn alphafold_file_url(accession: &str, pdb: bool) -> Result<String, ReqError> {
    let prediction = load_alphafold_predictions(accession)?
        .into_iter()
        .next()
        .ok_or(ReqError::Deserialize)?;

    let url = if pdb {
        prediction.pdb_url
    } else {
        prediction.cif_url
    };

    url.ok_or(ReqError::Deserialize)
}

/// Download a predicted structure from AlphaFold DB as an mmCIF string. For a protein modelled in
/// several fragments, this is the first of them; use `load_alphafold_predictions` to get them all.
///
/// Note that the B-factor column of an AlphaFold model holds per-residue pLDDT confidence, from
/// 0-100, rather than a crystallographic B-factor.
pub fn load_alphafold_cif(accession: &str) -> Result<String, ReqError> {
    get(&alphafold_file_url(accession, false)?)
}

/// Download a predicted structure from AlphaFold DB as a PDB string. See `load_alphafold_cif`
/// regarding fragments and the B-factor column.
pub fn load_alphafold_pdb(accession: &str) -> Result<String, ReqError> {
    get(&alphafold_file_url(accession, true)?)
}
