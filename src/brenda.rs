//! [Home page](https://www.brenda-enzymes.org/)
//! [SPARQL endpoint](https://sparql.dsmz.de/brenda)
//! [SOAP documentation](https://www.brenda-enzymes.org/soap.php)
//!
//! BRENDA curates enzyme reactions, substrate scope, organisms, cofactors, inhibitors, kinetics,
//! stability, process conditions, and engineered variants. This module uses its public DSMZ
//! Digital Diversity knowledge graph. The graph is currently labelled a prototype and exposes a
//! smaller surface than the credentialed SOAP service, but it has a standards-based JSON API and
//! is the appropriate unauthenticated integration point.
//!
//! `query_sparql` exposes the graph without constraining callers to today's schema. The typed
//! helpers cover the pieces most useful to synthesis: EC metadata, reaction participants and
//! compounds acting as inhibitors, activators, or cofactors. For kinetic measurements and protein
//! engineering records that have not yet reached the graph, use BRENDA's registered SOAP service;
//! it requires a user's own email and password and asks clients to stay below one request/second.

use std::collections::HashMap;

use serde::Deserialize;

use crate::{ReqError, make_agent};

const HOME_URL: &str = "https://www.brenda-enzymes.org";
const SPARQL_URL: &str = "https://sparql.dsmz.de/api/brenda";
const BRENDA_IRI: &str = "https://purl.dsmz.de/brenda";
const D3O_IRI: &str = "https://purl.dsmz.de/schema/";

const USER_AGENT: &str = concat!(
    "bio_apis/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/David-OConnor/bio_apis)"
);

/// One value in a W3C SPARQL JSON result binding.
#[derive(Clone, Debug, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct SparqlValue {
    /// Usually "uri" or "literal".
    #[serde(rename = "type")]
    pub type_: String,
    pub value: String,
    #[serde(rename = "xml:lang")]
    pub language: Option<String>,
    pub datatype: Option<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SparqlHead {
    pub vars: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SparqlBindings {
    pub bindings: Vec<HashMap<String, SparqlValue>>,
}

/// A standards-compatible SPARQL SELECT response.
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(default)]
pub struct SparqlResults {
    pub head: SparqlHead,
    pub results: SparqlBindings,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct EnzymeClass {
    /// EC number without an "EC:" prefix.
    pub ec_number: String,
    pub recommended_name: String,
    pub systematic_name: Option<String>,
    pub description: Option<String>,
    pub synonyms: Vec<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParticipantRole {
    Substrate,
    Product,
}

/// One compound on one side of a BRENDA reaction.
#[derive(Clone, Debug, PartialEq)]
pub struct ReactionParticipant {
    /// Stable BRENDA reaction IRI.
    pub reaction: String,
    pub role: ParticipantRole,
    /// Numeric BRENDA compound identifier, when present in the IRI.
    pub compound_id: Option<u32>,
    pub name: String,
    pub inchikey: Option<String>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum EffectorRole {
    Activator,
    Inhibitor,
    Cofactor,
}

impl EffectorRole {
    fn d3o_name(self) -> &'static str {
        match self {
            Self::Activator => "Activator",
            Self::Inhibitor => "Inhibitor",
            Self::Cofactor => "Cofactor",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Effector {
    pub role: EffectorRole,
    pub compound_id: Option<u32>,
    pub name: String,
    pub inchikey: Option<String>,
}

fn binding_value<'a>(binding: &'a HashMap<String, SparqlValue>, key: &str) -> Option<&'a str> {
    binding.get(key).map(|v| v.value.as_str())
}

fn id_from_iri(iri: &str) -> Option<u32> {
    iri.rsplit('/').next()?.parse().ok()
}

fn inchikey_bare(value: &str) -> Option<String> {
    let value = value.strip_prefix("InChIKey=").unwrap_or(value);
    (!value.is_empty()).then(|| value.to_owned())
}

fn ec_bare(ec_number: &str) -> Result<String, ReqError> {
    let ec = ec_number
        .trim()
        .trim_start_matches("EC:")
        .trim_start_matches("ec:");

    // The EC value becomes part of an IRI, so reject characters that could alter the SPARQL.
    if ec.is_empty()
        || !ec
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '.' | '-' | 'B'))
    {
        return Err(ReqError::Deserialize);
    }
    Ok(ec.to_owned())
}

/// Open BRENDA's browser search for an EC number.
pub fn open_overview(ec_number: &str) -> Result<(), ReqError> {
    let ec = ec_bare(ec_number)?;
    webbrowser::open(&format!("{HOME_URL}/enzyme.php?ecno={ec}"))?;
    Ok(())
}

/// Execute an arbitrary SELECT query against BRENDA's public QLever SPARQL endpoint.
///
/// Results use the standard `application/sparql-results+json` representation. Add an explicit
/// `LIMIT` to expensive queries; the endpoint itself may cap the returned rows.
pub fn query_sparql(query: &str) -> Result<SparqlResults, ReqError> {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    params.append_pair("query", query);
    // QLever otherwise sends only its UI default. A SPARQL LIMIT remains the authoritative cap.
    params.append_pair("send", "10000");

    let url = format!("{SPARQL_URL}?{}", params.finish());
    let mut resp = make_agent()
        .get(&url)
        .header("User-Agent", USER_AGENT)
        .header("Accept", "application/sparql-results+json")
        .call()?;

    if resp.status() != 200 {
        return Err(ReqError::Http);
    }

    Ok(serde_json::from_str(&resp.body_mut().read_to_string()?)?)
}

/// Load BRENDA's name, description, systematic name, and synonyms for an EC class.
pub fn load_enzyme_class(ec_number: &str) -> Result<EnzymeClass, ReqError> {
    let ec = ec_bare(ec_number)?;
    let query = format!(
        r#"PREFIX d3o: <{D3O_IRI}>
PREFIX dcterms: <http://purl.org/dc/terms/>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT ?name ?systematicName ?description ?synonym WHERE {{
  VALUES ?ec {{ <{BRENDA_IRI}/ec/{ec}> }}
  ?ec a d3o:ECNumber ; rdfs:label ?name .
  OPTIONAL {{ ?ec d3o:hasSystematicName ?systematicName . }}
  OPTIONAL {{ ?ec dcterms:description ?description . }}
  OPTIONAL {{ ?ec d3o:hasSynonyms ?synonym . }}
}}"#
    );

    let rows = query_sparql(&query)?.results.bindings;
    let first = rows.first().ok_or(ReqError::Deserialize)?;
    let mut result = EnzymeClass {
        ec_number: ec,
        recommended_name: binding_value(first, "name").unwrap_or_default().to_owned(),
        systematic_name: binding_value(first, "systematicName").map(str::to_owned),
        description: binding_value(first, "description").map(str::to_owned),
        synonyms: Vec::new(),
    };

    for row in rows {
        if let Some(value) = binding_value(&row, "synonym")
            && !result.synonyms.iter().any(|v| v == value)
        {
            result.synonyms.push(value.to_owned());
        }
    }
    Ok(result)
}

/// Return substrate and product records for reactions assigned to an EC number.
///
/// These are row-level records because a compound may appear in several organism- or
/// literature-specific BRENDA reactions for the same EC class. Group by `reaction` to reconstruct
/// each reaction's two sides.
pub fn reaction_participants_from_ec(
    ec_number: &str,
    limit: Option<u32>,
) -> Result<Vec<ReactionParticipant>, ReqError> {
    let ec = ec_bare(ec_number)?;
    let limit = limit.unwrap_or(10_000);
    let query = format!(
        r#"PREFIX d3o: <{D3O_IRI}>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT DISTINCT ?reaction ?roleType ?compound ?compoundName ?inchiKey WHERE {{
  ?reaction a d3o:Reaction ; d3o:catalyzedByClass <{BRENDA_IRI}/ec/{ec}> .
  ?role d3o:partOf ?reaction ; d3o:refersToCompound ?compound ; a ?roleType .
  VALUES ?roleType {{ d3o:Substrate d3o:Product }}
  ?compound rdfs:label ?compoundName .
  OPTIONAL {{
    ?compound d3o:hasStructure ?structure .
    ?structure d3o:hasInChIKey ?inchiKey .
  }}
}} LIMIT {limit}"#
    );

    query_sparql(&query)?
        .results
        .bindings
        .into_iter()
        .map(|row| {
            let role_iri = binding_value(&row, "roleType").ok_or(ReqError::Deserialize)?;
            let role = if role_iri.ends_with("/Substrate") {
                ParticipantRole::Substrate
            } else if role_iri.ends_with("/Product") {
                ParticipantRole::Product
            } else {
                return Err(ReqError::Deserialize);
            };
            let compound = binding_value(&row, "compound").ok_or(ReqError::Deserialize)?;

            Ok(ReactionParticipant {
                reaction: binding_value(&row, "reaction")
                    .ok_or(ReqError::Deserialize)?
                    .to_owned(),
                role,
                compound_id: id_from_iri(compound),
                name: binding_value(&row, "compoundName")
                    .unwrap_or_default()
                    .to_owned(),
                inchikey: binding_value(&row, "inchiKey").and_then(inchikey_bare),
            })
        })
        .collect()
}

/// Find compounds recorded as activators, inhibitors, or cofactors for an EC class.
pub fn effectors_from_ec(
    ec_number: &str,
    role: EffectorRole,
    limit: Option<u32>,
) -> Result<Vec<Effector>, ReqError> {
    let ec = ec_bare(ec_number)?;
    let role_name = role.d3o_name();
    let limit = limit.unwrap_or(10_000);
    let relation = match role {
        EffectorRole::Activator | EffectorRole::Inhibitor => format!(
            "?effect d3o:affectsClass <{BRENDA_IRI}/ec/{ec}> ; d3o:refersToCompound ?compound ."
        ),
        EffectorRole::Cofactor => format!(
            "?effect d3o:isCofactorOf ?protein ; d3o:refersToCompound ?compound .\n  \
             ?protein d3o:isClassifiedAs <{BRENDA_IRI}/ec/{ec}> ."
        ),
    };
    let query = format!(
        r#"PREFIX d3o: <{D3O_IRI}>
PREFIX rdfs: <http://www.w3.org/2000/01/rdf-schema#>
SELECT DISTINCT ?compound ?compoundName ?inchiKey WHERE {{
  ?effect a d3o:{role_name} .
  {relation}
  ?compound rdfs:label ?compoundName .
  OPTIONAL {{
    ?compound d3o:hasStructure ?structure .
    ?structure d3o:hasInChIKey ?inchiKey .
  }}
}} LIMIT {limit}"#
    );

    query_sparql(&query)?
        .results
        .bindings
        .into_iter()
        .map(|row| {
            let compound = binding_value(&row, "compound").ok_or(ReqError::Deserialize)?;
            Ok(Effector {
                role,
                compound_id: id_from_iri(compound),
                name: binding_value(&row, "compoundName")
                    .unwrap_or_default()
                    .to_owned(),
                inchikey: binding_value(&row, "inchiKey").and_then(inchikey_bare),
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_ec_iri_components() {
        assert_eq!(ec_bare("EC:1.1.1.1").unwrap(), "1.1.1.1");
        assert!(ec_bare("1.1.1.1> } UNION {").is_err());
    }

    #[test]
    fn normalizes_inchikeys() {
        assert_eq!(
            inchikey_bare("InChIKey=BAWFJGJZGIEFAR-NNYOXOHSSA-O").as_deref(),
            Some("BAWFJGJZGIEFAR-NNYOXOHSSA-O")
        );
        assert_eq!(inchikey_bare(""), None);
    }
}
