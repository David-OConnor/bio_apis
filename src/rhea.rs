//! [Home page](https://www.rhea-db.org/)
//! [API docs](https://www.rhea-db.org/help/rest-api)
//!
//! Rhea is a curated knowledgebase of biochemical reactions. Its entries are *reactions*;
//! participants are ChEBI entities,
//! so `Reaction::participant_chebi_ids` feeds directly into the `chebi` module.
//!
//! Rhea's table API returns tab-separated values for a set of columns you choose; we parse those
//! into `Reaction`. Note that `Reaction::enzyme_count` is a count only — see `uniprot_ids` to get
//! the actual UniProtKB accessions.
//!
//! Note: Rhea asks that programs identify themselves via the User-Agent header, so we set one.
//!
//! Note: MDL CT files (RXN, RD) come from Rhea's ExPASy distribution site rather than
//! www.rhea-db.org, whose per-entry file URLs sit behind a browser challenge that a plain HTTP
//! client can't clear.

use std::fmt;
use std::fmt::Display;
use crate::{ReqError, make_agent};

const BASE_URL: &str = "https://www.rhea-db.org";

/// Rhea's official distribution site; see https://www.rhea-db.org/help/download.
const CT_FILE_URL: &str = "https://ftp.expasy.org/databases/rhea/ctfiles";

/// Beta reaction-SMILES distribution described by Rhea's download documentation.
const REACTION_SMILES_URL: &str =
    "https://ftp.expasy.org/databases/rhea/tsv/rhea-reaction-smiles.tsv";

const UNIPROT_URL: &str = "https://rest.uniprot.org/uniprotkb/search";

/// UniProt's per-page maximum on its search endpoint.
const UNIPROT_PAGE_SIZE: u32 = 500;

const USER_AGENT: &str = concat!(
    "bio_apis/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/David-OConnor/bio_apis)"
);

/// The columns available from the table API.
/// [Column list](https://www.rhea-db.org/help/rest-api)
#[derive(Clone, Copy, PartialEq)]
pub enum Column {
    /// Reaction identifier, with the `RHEA` prefix.
    RheaId,
    /// Textual description of the reaction equation.
    Equation,
    /// ChEBI names of the reaction participants.
    ChebiName,
    /// ChEBI identifiers of the reaction participants.
    ChebiId,
    /// EC numbers, with the `EC` prefix.
    Ec,
    /// The *number* of UniProtKB entries annotated with this reaction.
    Uniprot,
    /// GO identifier (with the `GO` prefix) and label.
    Go,
    /// PubMed identifiers, without prefix.
    Pubmed,
    XrefEcoCyc,
    XrefMetaCyc,
    XrefKegg,
    XrefReactome,
    XrefMCsa,
}

impl Display for Column {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let v = match self {
            Self::RheaId => "rhea-id",
            Self::Equation => "equation",
            Self::ChebiName => "chebi",
            Self::ChebiId => "chebi-id",
            Self::Ec => "ec",
            Self::Uniprot => "uniprot",
            Self::Go => "go",
            Self::Pubmed => "pubmed",
            Self::XrefEcoCyc => "reaction-xref(EcoCyc)",
            Self::XrefMetaCyc => "reaction-xref(MetaCyc)",
            Self::XrefKegg => "reaction-xref(KEGG)",
            Self::XrefReactome => "reaction-xref(Reactome)",
            Self::XrefMCsa => "reaction-xref(M-CSA)",
        };
        write!(f, "{v}")
    }
}

/// The columns `Reaction` is built from. Rhea returns them in the order requested, so the parser
/// indexes fields by position here.
const REACTION_COLUMNS: [Column; 13] = [
    Column::RheaId,
    Column::Equation,
    Column::ChebiName,
    Column::ChebiId,
    Column::Ec,
    Column::Uniprot,
    Column::Go,
    Column::Pubmed,
    Column::XrefKegg,
    Column::XrefMetaCyc,
    Column::XrefEcoCyc,
    Column::XrefReactome,
    Column::XrefMCsa,
];

/// Rhea assigns each reaction four consecutive identifiers: the master (undirected) reaction,
/// then its left-to-right, right-to-left, and bidirectional variants. Searches index the master
/// only, while MDL CT files exist for the two directional variants only; this picks between them.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Direction {
    LeftToRight,
    RightToLeft,
}

/// The direction attached to a member of a Rhea reaction quartet.
///
/// Unlike [`Direction`], which selects one of the two connection-table files Rhea distributes,
/// this includes the undefined master reaction and the bidirectional (equilibrium) variant used
/// by database annotations.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub enum ReactionDirection {
    #[default]
    Undefined,
    LeftToRight,
    RightToLeft,
    Bidirectional,
}

impl ReactionDirection {
    /// Parse the names used by UniProt's `physiologicalReactions.directionType` field.
    pub fn from_name(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "undefined" => Some(Self::Undefined),
            "left-to-right" => Some(Self::LeftToRight),
            "right-to-left" => Some(Self::RightToLeft),
            "bidirectional" => Some(Self::Bidirectional),
            _ => None,
        }
    }
}

/// All four identifiers Rhea assigns to one chemistry.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct ReactionIds {
    pub master: u32,
    pub left_to_right: u32,
    pub right_to_left: u32,
    pub bidirectional: u32,
}

impl ReactionIds {
    pub const fn new(master: u32) -> Self {
        Self {
            master,
            left_to_right: master + 1,
            right_to_left: master + 2,
            bidirectional: master + 3,
        }
    }

    /// Classify one identifier from this quartet.
    pub const fn direction_for_id(&self, id: u32) -> Option<ReactionDirection> {
        if id == self.master {
            Some(ReactionDirection::Undefined)
        } else if id == self.left_to_right {
            Some(ReactionDirection::LeftToRight)
        } else if id == self.right_to_left {
            Some(ReactionDirection::RightToLeft)
        } else if id == self.bidirectional {
            Some(ReactionDirection::Bidirectional)
        } else {
            None
        }
    }
}

/// RDKit reaction SMILES for the two directed forms that Rhea distributes.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct ReactionSmiles {
    pub master_id: u32,
    pub left_to_right: Option<String>,
    pub right_to_left: Option<String>,
}

impl Direction {
    /// The identifier of this directional variant of a master reaction.
    pub fn id(&self, master_id: u32) -> u32 {
        match self {
            Self::LeftToRight => master_id + 1,
            Self::RightToLeft => master_id + 2,
        }
    }
}

/// A Gene Ontology molecular-function term for a reaction.
#[derive(Clone, Debug, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct GoTerm {
    /// E.g. "GO:0034875". Kept prefixed, as GO ids are zero-padded.
    pub id: String,
    /// E.g. "caffeine oxidase activity".
    pub label: String,
}

/// The participants on one side of a Rhea equation.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct ReactionSide {
    pub participant_names: Vec<String>,
    /// ChEBI identifiers only. A side may also contain a generic `RHEA-COMP` participant, which
    /// is retained in the combined participant fields on [`Reaction`] but has no ChEBI id here.
    pub participant_chebi_ids: Vec<u32>,
}

/// A Rhea reaction. Identifiers here are stored without their database prefixes.
#[derive(Clone, Debug, Default, PartialEq)]
#[cfg_attr(feature = "encode", derive(bincode::Encode, bincode::Decode))]
pub struct Reaction {
    /// The numeric portion of the master identifier, e.g. 10280 for RHEA:10280.
    pub id: u32,
    /// E.g. "caffeine + NADH + O2 + H(+) = theobromine + formaldehyde + NAD(+) + H2O".
    pub equation: String,
    /// ChEBI names of the participants, on both sides of the equation.
    pub participant_names: Vec<String>,
    /// ChEBI ids of the participants, e.g. 27732. Pass these to `chebi::load_compound`.
    pub participant_chebi_ids: Vec<u32>,
    /// Participants to the left of the equation symbol, in equation order.
    pub reactants: ReactionSide,
    /// Participants to the right of the equation symbol, in equation order.
    pub products: ReactionSide,
    /// E.g. "1.17.5.2".
    pub ec_numbers: Vec<String>,
    /// How many UniProtKB entries are annotated with this reaction. See `uniprot_ids` for the
    /// accessions themselves.
    pub enzyme_count: u32,
    pub go: Option<GoTerm>,
    pub pubmed_ids: Vec<u32>,
    /// E.g. "R07980".
    pub kegg: Vec<String>,
    /// E.g. "RXN-11523".
    pub metacyc: Vec<String>,
    pub ecocyc: Vec<String>,
    pub reactome: Vec<String>,
    /// Mechanism and Catalytic Site Atlas.
    pub m_csa: Vec<String>,
}

impl Reaction {
    /// Numeric M-CSA entry identifiers linked directly from this Rhea reaction.
    ///
    /// Rhea serializes these cross-references as values such as `M0283`, whereas the M-CSA API
    /// accepts the numeric portion. Keeping the conversion here avoids every downstream caller
    /// having to know both databases' identifier conventions.
    pub fn mcsa_ids(&self) -> Vec<u32> {
        self.m_csa
            .iter()
            .filter_map(|id| {
                id.trim()
                    .trim_start_matches("M-CSA:")
                    .trim_start_matches('M')
                    .parse()
                    .ok()
            })
            .collect()
    }

    /// Load M-CSA mechanism entries for this reaction.
    ///
    /// A direct Rhea/M-CSA cross-reference is preferred. Older or less completely cross-linked
    /// Rhea records fall back to their EC numbers, which is broader and should therefore be
    /// treated as family-level evidence rather than proof of an identical reaction.
    pub fn mcsa_entries(
        &self,
        fallback_limit: Option<u32>,
    ) -> Result<Vec<crate::mcsa::Entry>, ReqError> {
        let ids = self.mcsa_ids();
        if ids.is_empty() {
            crate::mcsa::entries_from_ec(&self.ec_numbers, fallback_limit)
        } else {
            crate::mcsa::entries_from_ids(&ids)
        }
    }

    /// Format the useful part of a Rhea reaction without dumping the whole record.
    pub fn format_simple(&self) -> String {
        format!("{}: {}", self.accession(), self.equation)
    }
}

impl Reaction {
    /// E.g. "RHEA:10280".
    pub fn accession(&self) -> String {
        format!("RHEA:{}", self.id)
    }
}

/// Split a semicolon-separated column, dropping each value's prefix, e.g. `EC:`.
fn split_col(field: &str, prefix: &str) -> Vec<String> {
    field
        .split(';')
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(|v| v.strip_prefix(prefix).unwrap_or(v).to_owned())
        .collect()
}

/// As `split_col`, but for numeric identifiers. Values that don't carry the prefix are skipped;
/// e.g. the participants column can include RHEA-COMP entries alongside ChEBI ones.
fn split_col_num(field: &str, prefix: &str) -> Vec<u32> {
    field
        .split(';')
        .map(str::trim)
        .filter_map(|v| v.strip_prefix(prefix))
        .filter_map(|v| v.parse().ok())
        .collect()
}

fn reaction_sides(
    equation: &str,
    participant_names: &[String],
    participant_identifiers: &[String],
) -> Result<(ReactionSide, ReactionSide), ReqError> {
    let (left, _) = equation.split_once(" = ").ok_or(ReqError::Deserialize)?;
    let left_count = left.split(" + ").count();

    if participant_names.len() != participant_identifiers.len()
        || left_count > participant_names.len()
    {
        return Err(ReqError::Deserialize);
    }

    let side = |range: std::ops::Range<usize>| ReactionSide {
        participant_names: participant_names[range.clone()].to_vec(),
        participant_chebi_ids: participant_identifiers[range]
            .iter()
            .filter_map(|id| id.strip_prefix("CHEBI:"))
            .filter_map(|id| id.parse().ok())
            .collect(),
    };

    Ok((
        side(0..left_count),
        side(left_count..participant_names.len()),
    ))
}

/// Parse the TSV table returned for `REACTION_COLUMNS`.
fn parse_reactions(tsv: &str) -> Result<Vec<Reaction>, ReqError> {
    let mut result = Vec::new();

    // The first line is the human-readable column header.
    for line in tsv.lines().skip(1) {
        if line.trim().is_empty() {
            continue;
        }

        let cols: Vec<&str> = line.split('\t').collect();
        if cols.len() < REACTION_COLUMNS.len() {
            return Err(ReqError::Deserialize);
        }

        let id = cols[0]
            .trim()
            .strip_prefix("RHEA:")
            .and_then(|v| v.parse().ok())
            .ok_or(ReqError::Deserialize)?;

        let go = cols[6].trim().split_once(' ').map(|(id, label)| GoTerm {
            id: id.to_owned(),
            label: label.to_owned(),
        });

        let equation = cols[1].trim().to_owned();
        let participant_names = split_col(cols[2], "");
        let participant_identifiers = split_col(cols[3], "");
        let (reactants, products) =
            reaction_sides(&equation, &participant_names, &participant_identifiers)?;

        result.push(Reaction {
            id,
            equation,
            participant_names,
            participant_chebi_ids: split_col_num(cols[3], "CHEBI:"),
            reactants,
            products,
            ec_numbers: split_col(cols[4], "EC:"),
            enzyme_count: cols[5].trim().parse().unwrap_or_default(),
            go,
            pubmed_ids: split_col_num(cols[7], ""),
            kegg: split_col(cols[8], "KEGG:"),
            metacyc: split_col(cols[9], "MetaCyc:"),
            ecocyc: split_col(cols[10], "EcoCyc:"),
            reactome: split_col(cols[11], "Reactome:"),
            m_csa: split_col(cols[12], "M-CSA:"),
        });
    }

    Ok(result)
}

/// Rhea asks that programs identify themselves, so we set a User-Agent. We also ask for an
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

/// Our agent doesn't treat error status codes as errors. A malformed Rhea query answers 500 with
/// an HTML page, which we'd otherwise hand back to the caller as if it were data.
fn get(url: &str) -> Result<String, ReqError> {
    let mut resp = request(url)?;

    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    Ok(resp.body_mut().read_to_string()?)
}

pub fn open_overview(id: u32) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/rhea/{id}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}

/// Calls the [table API](https://www.rhea-db.org/help/rest-api), returning raw TSV: a header row,
/// then one row per reaction, with the columns in the order requested.
///
/// The query string uses the same syntax as the website's search box, e.g. `caffeine`,
/// `rhea:10280`, `chebi:27732`, `ec:2.1.1.160`, `uniprot:Q9FLN8`, or `uniprot:*` for every
/// reaction with a curated enzyme. An empty query returns the whole data set, so pass a `limit`
/// unless you mean it.
pub fn query_table(
    query: &str,
    columns: &[Column],
    limit: Option<u32>,
) -> Result<String, ReqError> {
    let cols: Vec<String> = columns.iter().map(|c| c.to_string()).collect();

    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("query", query);
    params.append_pair("columns", &cols.join(","));
    params.append_pair("format", "tsv");

    if let Some(l) = limit {
        params.append_pair("limit", &l.to_string());
    }

    get(&format!("{BASE_URL}/rhea/?{}", params.finish()))
}

/// Search for reactions. See `query_table` for the query syntax.
pub fn search(query: &str, limit: Option<u32>) -> Result<Vec<Reaction>, ReqError> {
    parse_reactions(&query_table(query, &REACTION_COLUMNS, limit)?)
}

/// Load a list of Rhea ids from a search. Analogous to `pubchem::find_cids_from_search`.
pub fn find_ids_from_search(query: &str, limit: Option<u32>) -> Result<Vec<u32>, ReqError> {
    let tsv = query_table(query, &[Column::RheaId], limit)?;

    Ok(tsv
        .lines()
        .skip(1)
        .filter_map(|l| l.trim().strip_prefix("RHEA:"))
        .filter_map(|v| v.parse().ok())
        .collect())
}

/// Load a single reaction by its master id. Note that the search index holds master reactions
/// only; the three directional variants aren't retrievable this way, as they share the master's
/// data.
pub fn load_reaction(id: u32) -> Result<Reaction, ReqError> {
    search(&format!("rhea:{id}"), Some(1))?
        .into_iter()
        .next()
        .ok_or(ReqError::Deserialize)
}

/// Find the reactions a molecule participates in, from its ChEBI id. This is the main bridge from
/// the `chebi` module.
pub fn reactions_from_chebi(chebi_id: u32, limit: Option<u32>) -> Result<Vec<Reaction>, ReqError> {
    search(&format!("chebi:{chebi_id}"), limit)
}

/// Find reactions containing exactly this ChEBI entity, without expanding through ChEBI's class
/// and relationship hierarchy.
pub fn reactions_from_chebi_exact(
    chebi_id: u32,
    limit: Option<u32>,
) -> Result<Vec<Reaction>, ReqError> {
    search(&format!("chebi_exact:{chebi_id}"), limit)
}

/// Find reactions containing a compound by a full or partial InChIKey.
pub fn reactions_from_inchikey(
    inchikey: &str,
    limit: Option<u32>,
) -> Result<Vec<Reaction>, ReqError> {
    search(
        &format!(
            "inchikey:{}",
            inchikey.trim().trim_start_matches("InChIKey=")
        ),
        limit,
    )
}

/// Find the reactions catalysed by an enzyme class, e.g. "2.1.1.160". A partial EC number, e.g.
/// "2.1.1.-", also works.
pub fn reactions_from_ec(ec: &str, limit: Option<u32>) -> Result<Vec<Reaction>, ReqError> {
    search(&format!("ec:{}", ec.trim_start_matches("EC:")), limit)
}

/// Find the reactions a protein is annotated with, from its UniProtKB accession, e.g. "Q9FLN8".
pub fn reactions_from_uniprot(
    accession: &str,
    limit: Option<u32>,
) -> Result<Vec<Reaction>, ReqError> {
    search(&format!("uniprot:{accession}"), limit)
}

/// Find Rhea reactions linked to an M-CSA/MACiE mechanism identifier, e.g. "M0283".
pub fn reactions_from_mcsa(mcsa_id: &str, limit: Option<u32>) -> Result<Vec<Reaction>, ReqError> {
    search(
        &format!("macie:{}", mcsa_id.trim().trim_start_matches("M-CSA:")),
        limit,
    )
}

/// Search only approved reactions, which Rhea has checked for mass and charge balance.
pub fn approved_reactions(query: &str, limit: Option<u32>) -> Result<Vec<Reaction>, ReqError> {
    let query = query.trim();
    let query = if query.is_empty() {
        "status:approved".to_owned()
    } else {
        format!("({query}) AND status:approved")
    };
    search(&query, limit)
}

fn ct_file_url(id: u32, ext: &str) -> String {
    format!("{CT_FILE_URL}/{ext}/{id}.{ext}")
}

/// Download an MDL RXN file for one direction of a reaction, returning an RXN string. Each `$MOL`
/// block within is a participant's connection table, in 2D.
///
/// Note that these exist for directional variants only, which is why a `Direction` is required:
/// the MDL CT formats can't express a bidirectional or undefined-direction reaction.
pub fn load_rxn(master_id: u32, direction: Direction) -> Result<String, ReqError> {
    get(&ct_file_url(direction.id(master_id), "rxn"))
}

/// Download an MDL RD (reaction data) file for one direction of a reaction. This is an RXN plus
/// Rhea's data fields. See `load_rxn` regarding directions.
pub fn load_rd(master_id: u32, direction: Direction) -> Result<String, ReqError> {
    get(&ct_file_url(direction.id(master_id), "rd"))
}

/// Parse Rhea's bulk reaction-SMILES TSV and retain one master reaction.
///
/// The file has no header. Its first column is a left-to-right or right-to-left directional Rhea
/// id; its second is reaction SMILES. Rows for unrelated reactions are ignored.
pub fn parse_reaction_smiles(tsv: &str, master_id: u32) -> Result<ReactionSmiles, ReqError> {
    let ids = ReactionIds::new(master_id);
    let mut result = ReactionSmiles {
        master_id,
        ..Default::default()
    };

    for line in tsv.lines().map(str::trim).filter(|line| !line.is_empty()) {
        let Some((id, smiles)) = line.split_once('\t') else {
            return Err(ReqError::Deserialize);
        };
        let id: u32 = id.parse().map_err(|_| ReqError::Deserialize)?;
        if id == ids.left_to_right {
            result.left_to_right = Some(smiles.to_owned());
        } else if id == ids.right_to_left {
            result.right_to_left = Some(smiles.to_owned());
        }

        if result.left_to_right.is_some() && result.right_to_left.is_some() {
            break;
        }
    }

    if result.left_to_right.is_none() && result.right_to_left.is_none() {
        return Err(ReqError::Deserialize);
    }
    Ok(result)
}

/// Download Rhea's beta reaction-SMILES table and return the two directed representations for one
/// master reaction. For repeated or bulk use, download the table once and call
/// `parse_reaction_smiles` for each reaction.
pub fn load_reaction_smiles(master_id: u32) -> Result<ReactionSmiles, ReqError> {
    parse_reaction_smiles(&get(REACTION_SMILES_URL)?, master_id)
}

/// Load full UniProt records for proteins annotated as catalysing a reaction.
///
/// This is the richer counterpart of `uniprot_ids`, and keeps Rhea and UniProt's query behavior in
/// one place. `exclude_fragments` is useful when building an enzyme-expression candidate panel.
pub fn proteins(
    master_id: u32,
    reviewed_only: bool,
    exclude_fragments: bool,
    fields: &[crate::uniprot::Field],
    limit: Option<u32>,
) -> Result<Vec<crate::uniprot::Protein>, ReqError> {
    crate::uniprot::proteins_from_rhea_filtered(
        master_id,
        reviewed_only,
        exclude_fragments,
        fields,
        limit,
    )
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

/// Find the proteins annotated with a reaction, as UniProtKB accessions, e.g. "Q9FZN8".
/// `Reaction::enzyme_count` carries how many there are without a second request; check it before
/// calling this, as well-studied reactions have thousands.
///
/// Rhea stores that count only, so this queries UniProt's REST API, in the manner Rhea's API docs
/// document. If `reviewed_only` is true, results are limited to UniProtKB/Swiss-Prot. `limit`
/// caps the number returned; `None` walks every page.
pub fn uniprot_ids(
    master_id: u32,
    reviewed_only: bool,
    limit: Option<u32>,
) -> Result<Vec<String>, ReqError> {
    let mut query = format!("(cc_catalytic_activity:\"rhea:{master_id}\")");
    if reviewed_only {
        query += " AND (reviewed:true)";
    }

    let page_size = limit.unwrap_or(UNIPROT_PAGE_SIZE).min(UNIPROT_PAGE_SIZE);

    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("query", &query);
    params.append_pair("fields", "accession");
    params.append_pair("format", "tsv");
    params.append_pair("size", &page_size.to_string());

    let mut url = Some(format!("{UNIPROT_URL}?{}", params.finish()));
    let mut result = Vec::new();

    // UniProt pages via a cursor it hands back in the `Link` header.
    while let Some(u) = url {
        let mut resp = request(&u)?;

        if resp.status() != 200 {
            return Err(ReqError::Http);
        }

        url = resp
            .headers()
            .get("link")
            .and_then(|v| v.to_str().ok())
            .and_then(parse_next_link);

        let tsv = resp.body_mut().read_to_string()?;

        // The first line is the column header.
        let accessions: Vec<String> = tsv
            .lines()
            .skip(1)
            .map(str::trim)
            .filter(|l| !l.is_empty())
            .map(str::to_owned)
            .collect();

        // Guard against a `next` link that doesn't advance.
        if accessions.is_empty() {
            break;
        }

        result.extend(accessions);

        if let Some(l) = limit
            && result.len() >= l as usize
        {
            result.truncate(l as usize);
            break;
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_directed_reaction_smiles() {
        let tsv = "10001\tA>>B\n10002\tB>>A\n10005\tC>>D\n";
        let result = parse_reaction_smiles(tsv, 10000).unwrap();
        assert_eq!(result.left_to_right.as_deref(), Some("A>>B"));
        assert_eq!(result.right_to_left.as_deref(), Some("B>>A"));
        assert_eq!(ReactionIds::new(10000).bidirectional, 10003);
    }

    #[test]
    fn parses_reaction_participants_by_side() {
        let tsv = concat!(
            "Reaction identifier\tEquation\tChEBI name\tChEBI identifier\tEC number\tEnzymes\tGO molecular function\tPubMed\tKEGG\tMetaCyc\tEcoCyc\tReactome\tM-CSA\n",
            "RHEA:27902\tubiquinone-0 + caffeine + H2O = ubiquinol-0 + 1,3,7-trimethylurate\t",
            "ubiquinone-0;caffeine;water;ubiquinol-0;1,3,7-trimethyluric acid\t",
            "CHEBI:27906;CHEBI:27732;CHEBI:15377;CHEBI:60899;CHEBI:691622\t",
            "EC:1.17.5.2\t3\t\t17981969\t\t\t\t\t\n",
        );

        let reaction = parse_reactions(tsv).unwrap().remove(0);
        assert_eq!(
            reaction.reactants.participant_chebi_ids,
            [27906, 27732, 15377]
        );
        assert_eq!(reaction.products.participant_chebi_ids, [60899, 691622]);
        assert_eq!(reaction.participant_chebi_ids.len(), 5);
    }

    #[test]
    fn classifies_all_quartet_directions() {
        let ids = ReactionIds::new(27902);
        assert_eq!(
            ids.direction_for_id(27902),
            Some(ReactionDirection::Undefined)
        );
        assert_eq!(
            ids.direction_for_id(27903),
            Some(ReactionDirection::LeftToRight)
        );
        assert_eq!(
            ids.direction_for_id(27904),
            Some(ReactionDirection::RightToLeft)
        );
        assert_eq!(
            ids.direction_for_id(27905),
            Some(ReactionDirection::Bidirectional)
        );
        assert_eq!(ids.direction_for_id(12345), None);
    }

    #[test]
    fn normalizes_mcsa_cross_references() {
        let reaction = Reaction {
            m_csa: vec!["M0283".to_owned(), "M-CSA:M0042".to_owned()],
            ..Default::default()
        };
        assert_eq!(reaction.mcsa_ids(), [283, 42]);
    }
}
