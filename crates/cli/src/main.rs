//! An imported build in, a rotation out. Nothing in this path touches the
//! network: the catalogue, the class mechanics and the spell figures are
//! committed.

mod serve;

use dofus_app::solve;

use std::process::ExitCode;

use solve::Request;

fn print_report(request: &Request) -> Result<(), String> {
    let outcome = solve::run(request)?;
    let r = &outcome.resolved;

    println!("=== build ===");
    println!("  {} PA, {} PM", r.base_ap, r.base_mp);
    println!(
        "  {}% de coup critique venant de l'équipement",
        r.crit_bonus_percent
    );
    for (name, percent) in &r.damage_multipliers {
        let when = match r.tours.get(name) {
            Some(dofus_build::Tours::Impairs) => " (tours impairs)",
            Some(dofus_build::Tours::Pairs) => " (tours pairs)",
            None if request.odd_turns_only.iter().any(|o| o == name) => " (tours impairs seulement)",
            None => "",
        };
        println!("  {name} x{:.2}{when}", f64::from(*percent) / 100.0);
    }
    for (name, pieces) in &r.sets_active {
        println!("  {name}, {pieces} pièces");
    }
    if !r.uninterpreted.is_empty() {
        println!("\n=== non interprété ===");
        for u in &r.uninterpreted {
            println!("  {u}");
        }
    }
    println!("\n=== hypothèses ===");
    for a in &r.assumptions {
        println!("  {a}");
    }
    println!("\n=== rotation ===");
    println!("{}", outcome.solution);
    println!(
        "Soit {:.0} par tour",
        outcome.solution.total.as_f64() / f64::from(request.horizon.max(1))
    );
    let wasted: i16 = outcome.solution.turns.iter().map(|t| t.ap_left).sum();
    if wasted > 0 {
        println!("{wasted} PA non dépensés au total : il manque un sort bon marché au deck");
    }
    // Un total de zone repose sur une capacité que la donnée ne donne pas toujours :
    // le dire, plutôt que de laisser lire comme exact un chiffre qui peut dépasser
    // ce que la zone atteint. La dégressivité passe d'abord : elle touche presque
    // tous les sorts de zone.
    if !outcome.zones_degressives.is_empty() {
        println!(
            "\n⚠️  Dégâts de zone dégressifs pour {} : le jeu retire 10 % par case \
             d'éloignement du point d'impact, jusqu'à quatre fois. Ce calculateur \
             compte chaque cible à plein tarif, donc ce total suppose vos ennemis \
             collés à l'impact.",
            outcome.zones_degressives.join(", ")
        );
    }
    if !outcome.zones_sans_plafond.is_empty() {
        println!(
            "\n⚠️  Capacité de zone inconnue pour {} : leur forme n'est pas décodée, \
             donc ils suivent le nombre d'ennemis annoncé sans plafond. Au-delà de \
             la taille réelle de leur zone, ce total est trop haut.",
            outcome.zones_sans_plafond.join(", ")
        );
    }
    println!(
        "\nrésolu en {:.3}s sur {} états",
        outcome.seconds, outcome.solution.inter_turn_states
    );
    Ok(())
}

fn usage() {
    eprintln!("usage:");
    eprintln!("  dofus <build.json>     résout une rotation et l'affiche");
    eprintln!("  dofus serve [port]     ouvre l'interface locale (7878 par défaut)");
    eprintln!("  dofus gaps [classe]    ce qu'il reste à écrire, en tableau ou en détail");
    eprintln!("  dofus note <note.json> [--json]");
    eprintln!("                         une note de patch confrontée à la donnée, à blanc");
    eprintln!("  dofus note <note.json> --appliquer [--donnees data]");
    eprintln!("                         une note de SORTIE appliquée aux fichiers de la donnée");
    eprintln!("  dofus beta <note.json>...");
    eprintln!("                         les notes de la bêta, pour data/beta.json (onglet « Bêta en cours »)");
}

/// Les notes de la bêta, lues à blanc et rangées pour l'onglet « Bêta en
/// cours » : la sortie se range dans `data/beta.json`.
fn print_beta(chemins: &[String]) -> Result<(), String> {
    use dofus_app::notes::{beta::pour_l_onglet, Note};
    let notes = chemins
        .iter()
        .map(|c| {
            let texte = std::fs::read_to_string(c).map_err(|e| format!("{c}: {e}"))?;
            serde_json::from_str::<Note>(&texte).map_err(|e| format!("{c}: {e}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let d = pour_l_onglet(&notes);
    println!("{}", serde_json::to_string_pretty(&d).map_err(|e| e.to_string())?);
    Ok(())
}

/// Une note de SORTIE appliquée aux fichiers de la donnée, puis vérifiée.
/// La donnée étant compilée dans l'application, il faut la reconstruire ensuite.
fn appliquer_note(chemin: &str, dossier: &str) -> Result<(), String> {
    use dofus_app::notes::appliquer::{appliquer, aujourd_hui};
    use dofus_app::notes::Note;
    let dossier = std::path::Path::new(dossier);
    if !dossier.join("version.json").exists() {
        return Err(format!("{} : pas de version.json, est-ce le dossier data ?", dossier.display()));
    }
    let texte = std::fs::read_to_string(chemin).map_err(|e| format!("{chemin}: {e}"))?;
    let note: Note = serde_json::from_str(&texte).map_err(|e| format!("{chemin}: {e}"))?;
    let fait = appliquer(&note, dossier, &aujourd_hui())?;
    let nom_classe = |id: u32| solve::CLASSES.iter().find(|c| c.0 == id).map_or("?", |c| c.2);
    println!("Note {} « {} » : {} ligne(s) appliquée(s)", note.id, note.titre, fait.appliquees.len());
    for l in &fait.appliquees {
        println!("  [{}] {} : {}", nom_classe(l.classe), l.sorts.join(", "), l.texte);
    }
    if !fait.laissees.is_empty() {
        println!("\nLaissées :");
        for (l, raison) in &fait.laissees {
            println!("  [{}] {} : {}  ({raison})", nom_classe(l.classe), l.sorts.join(", "), l.texte);
        }
    }
    if !fait.fichiers.is_empty() {
        println!("\nFichiers modifiés :");
        for f in &fait.fichiers {
            println!("  {}", f.display());
        }
        println!("La donnée est compilée dans l'application : reconstruisez-la.");
    }
    Ok(())
}

/// Une note de patch confrontée à la donnée, sans rien écrire : ce qui reste à
/// appliquer, ce qui l'est déjà, les écarts, et la prose à modéliser. La note
/// est le JSON d'une `notes::Note`.
fn print_note(chemin: &str, json: bool) -> Result<(), String> {
    use dofus_app::notes::{lire, Etat, Note};
    let texte = std::fs::read_to_string(chemin).map_err(|e| format!("{chemin}: {e}"))?;
    let note: Note = serde_json::from_str(&texte).map_err(|e| format!("{chemin}: {e}"))?;
    let lignes = lire(&note);
    if json {
        println!("{}", serde_json::to_string_pretty(&lignes).map_err(|e| e.to_string())?);
        return Ok(());
    }
    let nom_classe = |id: u32| solve::CLASSES.iter().find(|c| c.0 == id).map_or("?", |c| c.2);
    let compte = |f: &dyn Fn(&Etat) -> bool| lignes.iter().filter(|l| f(&l.etat)).count();
    println!("Note {} « {} » : {} lignes de classe", note.id, note.titre, lignes.len());
    println!("  à appliquer  {:>4}", compte(&|e| *e == Etat::AAppliquer));
    println!("  déjà à jour  {:>4}", compte(&|e| *e == Etat::DejaAJour));
    println!("  écart        {:>4}", compte(&|e| matches!(e, Etat::Ecart { .. })));
    println!("  autre grade  {:>4}", compte(&|e| *e == Etat::AutreGrade));
    println!("  illisible    {:>4}", compte(&|e| *e == Etat::Illisible));
    println!("  prose        {:>4}", compte(&|e| *e == Etat::Prose));
    for (titre, garder) in [
        ("À appliquer", &(|e: &Etat| *e == Etat::AAppliquer) as &dyn Fn(&Etat) -> bool),
        ("Écarts", &|e: &Etat| matches!(e, Etat::Ecart { .. })),
        ("Illisibles", &|e: &Etat| *e == Etat::Illisible),
    ] {
        let choisies: Vec<_> = lignes.iter().filter(|l| garder(&l.etat)).collect();
        if choisies.is_empty() {
            continue;
        }
        println!("\n{titre} :");
        for l in choisies {
            let actuel = match &l.etat {
                Etat::Ecart { actuel } => format!("  (donnée : {actuel})"),
                _ => String::new(),
            };
            println!("  [{}] {} : {}{actuel}", nom_classe(l.classe), l.sorts.join(", "), l.texte);
        }
    }
    Ok(())
}

/// Ce qu'il reste à écrire, classe par classe ou pour une seule : la liste des
/// sorts partiels, pas un pourcentage.
fn print_gaps(classe: Option<&str>) -> Result<(), String> {
    let (mut faits, mut total, mut hors) = (0usize, 0usize, 0usize);
    for (id, nom, _) in solve::CLASSES {
        if classe.is_some_and(|c| c != *nom) {
            continue;
        }
        let ruleset = solve::load_ruleset(*id)?;
        let snapshot = solve::snapshot_for(*id)?;
        let mut rs = ruleset;
        rs.merge_snapshot(&snapshot);
        let c = snapshot.coverage(&rs);
        faits += c.fully_modelled;
        total += c.damaging;
        hors += c.outside_rotation.len();
        let dehors = match c.outside_rotation.len() {
            0 => String::new(),
            n => format!(", {n} hors rotation"),
        };
        println!(
            "{nom:12} {:>3}%  ({}/{} sorts complets{dehors})",
            c.percent_complete(),
            c.fully_modelled,
            c.damaging
        );
        if classe.is_none() {
            continue;
        }
        let partiels: std::collections::BTreeSet<&str> =
            c.partial.iter().map(|(n, _)| n.as_str()).collect();
        for gap in rs.data_gaps() {
            let id = gap.path.split('.').next().unwrap_or("");
            let Some(s) = rs.spells.iter().find(|s| s.id == id) else {
                continue;
            };
            if partiels.contains(s.name.fr.as_str()) {
                println!("  [{}] {}", s.name.fr, gap.what);
            }
        }
    }
    if total > 0 && classe.is_none() {
        println!(
            "\nTOTAL {faits}/{total} = {}%, et {hors} sorts hors rotation par decision",
            faits * 100 / total
        );
    }
    Ok(())
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().collect();
    let result = match args.get(1).map(String::as_str) {
        // `gaps` sans argument donne le tableau des dix-neuf classes, avec un
        // nom de classe il détaille ce qui manque à chacun de ses sorts.
        Some("gaps") => print_gaps(args.get(2).map(String::as_str)),
        Some("beta") if args.len() > 2 => print_beta(&args[2..]),
        Some("note") => match args.get(2) {
            Some(chemin) if args.iter().any(|a| a == "--appliquer") => {
                let dossier = args
                    .iter()
                    .position(|a| a == "--donnees")
                    .and_then(|i| args.get(i + 1))
                    .map_or("data", String::as_str);
                appliquer_note(chemin, dossier)
            }
            Some(chemin) => print_note(chemin, args.iter().any(|a| a == "--json")),
            None => {
                usage();
                return ExitCode::from(2);
            }
        },
        Some("serve") => {
            let port = args.get(2).and_then(|p| p.parse().ok()).unwrap_or(7878);
            serve::serve(port)
        }
        Some("-h") | Some("--help") | None => {
            usage();
            return ExitCode::from(2);
        }
        Some(path) => std::fs::read_to_string(path)
            .map_err(|e| format!("{path}: {e}"))
            .and_then(|t| serde_json::from_str::<Request>(&t).map_err(|e| format!("{path}: {e}")))
            .and_then(|r| print_report(&r)),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("erreur : {e}");
            ExitCode::FAILURE
        }
    }
}
