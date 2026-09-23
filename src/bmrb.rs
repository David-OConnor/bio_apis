//! [Home page](https://bmrb.io/)
//! [API docs](https://github.com/bmrb-io/BMRB-API)
//!
//! The Biological Magnetic Resonance Data Bank (BMRB) holds NMR data, e.g. chemical shifts, for
//! proteins and other biomolecules. NMR structures in the PDB often have an entry here.

const BASE_URL: &str = "https://bmrb.io/data_library/summary/index.php";

/// Open the page for an entry, given its ID, e.g. `30795`.
pub fn open_overview(id: &str) {
    if let Err(e) = webbrowser::open(&format!("{BASE_URL}?bmrbId={}", id.trim())) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}
