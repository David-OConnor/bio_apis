use na_seq::{Nucleotide, seq_to_str_lower};
use url::Url;

const BLAST_URL: &str = "https://blast.ncbi.nlm.nih.gov/Blast.cgi";

/// Open the web browser to a NCBI-BLAST page, of the sequence of interest.
///
///Example BLAST
/// note: There appears to be a NT limit that will fail most full plastmids when using the GET api.
/// ?PAGE_TYPE=BlastSearch&CMD=Web&LAYOUT=OneWindow&PROGRAM=blastn&MEGABLAST=on&PAGE=Nucleotides&DATABASE=nr
/// &FORMAT_TYPE=HTML&NCBI_GI=on&SHOW_OVERVIEW=on&QUERY=%3Ettt%20%20(43%20..%20905%20%3D%20863%20bp)
/// %0AACTCACTATAGGGAAATAATTTTGTTTAACTTTAAGAAGGAGATATACCGGTATGACTAGTATGGAAGACGCCAAAAACATAAAGAAAGGCCCG
/// GCGCCATTCTATCCGCTGGAAGATGGAACCGCTGGAGAGCAACTGCATAAGGCTATGAAGAGATACGCCCTGGTTCCTGGAACAATTGCTTTTACAGA
/// TGCACATATCGAGGTGGACATCACTTACGCTGAGTACTTCGAAATGTCCGTTCGGTTGGCAGAAGCTATGAAACGATATGGGCTGAATACAAATCACAGA
/// ATCGTCGTATGCAGTGAAAACTCTCTTCAATTCTTTATGCCGGTGTTGGGCGCGTTATTTATCGGAGTTGCAGTTGCGCCCGCGAACGACATTTATAATGA
/// ACGTGAATTGCTCAACAGTATGGGCATTTCGCAGCCTACCGTGGTGTTCGTTTCCAAAAAGGGGTTGCAAAAAATTTTGAACGTGCAAAAAAAGCTCCCAAT
/// CATCCAAAAAATTATTATCATGGATTCTAAAACGGATTACCAGGGATTTCAGTCGATGTACACGTTCGTCACATCTCATCTACCTCCCGGTTTTAATGAATAC
/// GATTTTGTGCCAGAGTCCTTCGATAGGGACAAGACAATTGCACTGATCATGAACTCCTCTGGATCTACTGGTCTGCCTAAAGGTGTCGCTCTGCCTCATAGAACT
/// GCCTGCGTGAGATTCTCGCATGCCAGAGATCCTATTTTTGGCAATCAAATCATTCCGGATACTGCGATTTTAAGTGTTGTTCCATTCCATCACGGTTTTGGAA
/// TGTTTACTACACTCGGATATTTGATATGTGGATTTCGAGTCGTCTTAATGTATAGAT
pub fn open_blast(seq: &[Nucleotide], seq_name: &str) {
    let text_query = format!(">{seq_name} ({} bp)\n{}", seq.len(), seq_to_str_lower(seq));

    let params = vec![
        ("PAGE_TYPE", "BlastSearch"),
        ("CMD", "Web"),
        ("LAYOUT", "OneWindow"),
        ("PROGRAM", "blastn"),
        ("MEGABLAST", "on"),
        ("PAGE", "Nucleotides"),
        ("DATABASE", "nr"),
        ("FORMAT_TYPE", "HTML"),
        ("NCBI_GI", "on"),
        ("SHOW_OVERVIEW", "on"),
        ("QUERY", &text_query),
    ];

    let mut url = Url::parse(BLAST_URL).unwrap();
    {
        let mut query_pairs = url.query_pairs_mut();
        for (key, value) in params {
            query_pairs.append_pair(key, value);
        }
    }

    // Open the URL in the default web browser
    if let Err(e) = webbrowser::open(url.as_str()) {
        eprintln!("Failed to open the web browser: {:?}", e);
    }
}
