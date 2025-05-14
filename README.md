# Biology APIs

[![Crate](https://img.shields.io/crates/v/bio_apis.svg)](https://crates.io/crates/bio_apis)
[![Docs](https://docs.rs/bio_apis/badge.svg)](https://docs.rs/bio_apis)


This library contains abstractions to interact with biology-related public HTTP APIs. It includes functionality related to the following:

It uses rigid data structures for requests and responses, and enums where possible to constrain API options.

## Example functionality:
  - Download molecule data in various formats (e.g. CIF, SDF)
  - Open your default web browser to a  molecule's overview page, 3D structure etc
  - Search APIs for molecule data, or filter and return a list of IDs.

WIP: Many features unsupported. Implementing as used by Daedelus and PlasCAD.

## API support
- [RCSB](https://data.rcsb.org/) (Protein data bank)
- [PubChem](https://pubchem.ncbi.nlm.nih.gov/docs/pug-rest)
- [DrugBank](https://docs.drugbank.com/v1/)


See the [API docs](https://docs.rs/bio_apis) for functionality.