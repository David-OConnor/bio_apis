//! [Home page](https://www.ebi.ac.uk/emdb/)
//! [API docs](https://www.ebi.ac.uk/emdb/api/)
//!
//! The Electron Microscopy Data Bank (EMDB) holds 3D maps from cryo-EM, and related techniques.
//! Many PDB entries are models built into one of these maps.

const BASE_URL: &str = "https://www.ebi.ac.uk/emdb";

/// Open the page for a map, given its accession, e.g. `EMD-21375`. A bare number, e.g. `21375`,
/// is also accepted.
pub fn open_overview(id: &str) {
    let id = id.trim();
    let id = match id.parse::<u32>() {
        Ok(num) => format!("EMD-{num}"),
        Err(_) => id.to_uppercase(),
    };

    if let Err(e) = webbrowser::open(&format!("{BASE_URL}/{id}")) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}
