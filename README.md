# Biology APIs

[![Crate](https://img.shields.io/crates/v/bio_apis.svg)](https://crates.io/crates/bio_apis)
[![Docs](https://docs.rs/bio_apis/badge.svg)](https://docs.rs/bio_apis)

[Home page](https://www.athanorlab.com/rust-tools)

This library contains abstractions to interact with public biology databases that have HTTP APIs. It uses rigid
data structures for requests and responses, and enums where possible to constrain API options.

## APIs supported
- [RCSB](https://data.rcsb.org/) (Protein data bank)
- [PubChem](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest)
- [PDBe](https://www.ebi.ac.uk/pdbe/)
- [DrugBank](https://docs.drugbank.com/v1/)
- [NCBI BLAST](https://blast.ncbi.nlm.nih.gov/Blast.cgi)
- [Rhea](https://www.rhea-db.org/)
- [UniProt](https://www.uniprot.org/)
- [ChEBI](https://www.ebi.ac.uk/chebi/)
- [LMSD](https://www.lipidmaps.org)
- [BRENDA](https://www.brenda-enzymes.org/) (public SPARQL knowledge graph)
- [M-CSA](https://www.ebi.ac.uk/thornton-srv/m-csa/) (enzyme mechanisms and catalytic sites)
- Mol2, FRCMOD, and Lib data for Amber Geostd organic molecules


## Example functionality:
  - Download molecule data in various formats (e.g. mmCIF, SDF)
  - Open your default web browser to a  molecule's overview page, 3D structure etc
  - Search APIs for molecule data, or filter and return a list of IDs.
  - Load information on a protein from the RCSB data API or UniProt
  - Load electron density data for a protein
  - Query reactions on Rhea, and load associated enzymes and small molecules
  - Download a molecule based on its identifier

Example of various API functionality:

```rust
let data = bio_apis::rcsb::get_all_data("1ba3")?;

let data = amber_geostd::find_mols(&lig.common.ident).unwrap();

let cif_text = rcsb::load_cif(ident).unwrap();

let sdf_data = drugbank::load_sdf(ident).unwrap();
let sdf_data = pubchem::load_sdf(ident).unwrap();
let mol2_data = amber_geostd::load_mol2(ident).unwrap();

let protein = uniprot::load_protein("P69905").unwrap();
let seq = protein.seq_aa();
let structures = protein.pdb_ids();

// A predicted structure, for proteins the PDB has no experimental one for.
let cif_text = uniprot::load_alphafold_cif("P69905").unwrap();

// Build a complete, reviewed enzyme-candidate panel for a Rhea reaction.
let fields = uniprot::enzyme_candidate_fields();
let candidates = rhea::proteins(10280, true, true, &fields, Some(50)).unwrap();

// Add active-site residue roles and stepwise mechanism annotations.
let mechanisms = mcsa::entries_from_uniprot("P56868", Some(10)).unwrap();

// Retrieve BRENDA substrate/product precedents for an enzyme class.
let precedents = brenda::reaction_participants_from_ec("1.1.1.1", Some(500)).unwrap();

pubchem::open_overview(ident);
```

We support flexible queries of the [Pubchem URL-based API](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest#section=URL-based-API) using the `pubchem::url_api_query()` function. Parameters
are passed as enums to pull various data from this flexble API. Example:

```rust
let resp = url_api_query(
    Domain::Compound,
    Namespace::Compound(NamespaceCompound::FastSearch((
        FastSearchCat::FastSimilarity3d,
        StructureSearchNamespace::Cid,
    ))),
    &[cid.to_string()],
    OperationSpecification::Compound(OpSpecCompound::Cids),
)?;
```

This returns a string, which can be further parsed based on the nature of the data. For example,
parsing into a structure or array using Serde, depending on the shape of the output for a given query.


See the [API docs](https://docs.rs/bio_apis) for functionality.
