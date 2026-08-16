//! Small live smoke test and starting point for a synthesis enzyme-discovery workflow.

use bio_apis::{brenda, mcsa, rhea, uniprot};

fn main() {
    let reaction = rhea::load_reaction(10280).expect("Rhea reaction");
    println!("RHEA:{}: {}", reaction.id, reaction.equation);

    let fields = uniprot::enzyme_candidate_fields();
    let candidates = rhea::proteins(reaction.id, true, true, &fields, Some(2))
        .expect("Rhea-to-UniProt candidates");
    for protein in candidates {
        println!(
            "  {}: {} ({})",
            protein.primary_accession,
            protein.name(),
            protein
                .organism
                .as_ref()
                .map(|o| o.scientific_name.as_str())
                .unwrap_or("unknown organism")
        );
    }

    let mechanism = mcsa::load_entry(1).expect("M-CSA entry");
    println!(
        "M-CSA {}: {} — {} catalytic residues",
        mechanism.mcsa_id,
        mechanism.enzyme_name,
        mechanism.residues.len()
    );

    let enzyme_class = brenda::load_enzyme_class("1.1.1.1").expect("BRENDA enzyme class");
    let participants =
        brenda::reaction_participants_from_ec("1.1.1.1", Some(10)).expect("BRENDA reactions");
    let inhibitors = brenda::effectors_from_ec("1.1.1.1", brenda::EffectorRole::Inhibitor, Some(5))
        .expect("BRENDA inhibitors");
    let cofactors = brenda::effectors_from_ec("1.1.1.1", brenda::EffectorRole::Cofactor, Some(5))
        .expect("BRENDA cofactors");
    println!(
        "BRENDA EC {} ({}): {} participant records shown",
        enzyme_class.ec_number,
        enzyme_class.recommended_name,
        participants.len()
    );
    println!("  {} inhibitor records shown", inhibitors.len());
    println!("  {} cofactor records shown", cofactors.len());
}
