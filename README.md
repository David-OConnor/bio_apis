# Biology APIs

[![Crate](https://img.shields.io/crates/v/bio_apis.svg)](https://crates.io/crates/bio_apis)
[![Docs](https://docs.rs/bio_apis/badge.svg)](https://docs.rs/bio_apis)


This library contains abstractions to interact with the public biology databases that hav HTTP APIs. It uses rigid
data structures for requests and responses, and enums where possible to constrain API options.

## Example functionality:
  - Download molecule data in various formats (e.g. CIF, SDF)
  - Open your default web browser to a  molecule's overview page, 3D structure etc
  - Search APIs for molecule data, or filter and return a list of IDs.
  - Load all information on a protein from the RCSB data API
  - Load electron density data for a protein.
  - Download a molecule based on its identifier.

Example of various API functionality:

```rust
let data = bio_apis::rcsb::get_all_data("1ba3")?;

let data = amber_geostd::find_mols(&lig.common.ident).unwrap();

let cif_text = rcsb::load_cif(ident).unwrap();

let sdf_data = drugbank::load_sdf(ident).unwrap();
let sdf_data = pubchem::load_sdf(ident).unwrap();

pubchem::open_overview(ident);
```


WIP: Many features unsupported. Implementing as used by Daedelus and PlasCAD.

## API support
- [RCSB](https://data.rcsb.org/) (Protein data bank)
- [PubChem](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest)
- [PDBe](https://www.ebi.ac.uk/pdbe/)
- [DrugBank](https://docs.drugbank.com/v1/)
- [NCBI BLAST](https://blast.ncbi.nlm.nih.gov/Blast.cgi)
- [LMSD](https://www.lipidmaps.org)
- Mol2, FRCMOD, and Lib data for Amber Geostd organic molecules

See the [API docs](https://docs.rs/bio_apis) for functionality.