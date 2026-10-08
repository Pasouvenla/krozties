//! Une note de sortie appliquée à la donnée : le relevé de chaque classe
//! (`data/snapshots/breed-N.json`) et, quand elles portent la valeur, ses
//! règles écrites à la main (`data/rulesets/<classe>.yaml`).
//!
//! * Jamais une note de bêta : elle se lit à blanc.
//! * Rien à l'aveugle : une valeur ne change que là où la donnée porte
//!   exactement l'avant de la note ; une règle écrite qui porte autre chose
//!   arrête la ligne.
//! * Les deux côtés ou aucun : le chargement refuse tout désaccord entre la
//!   règle et le relevé. Une ligne touche donc le relevé, à tous les grades
//!   qu'elle chiffre, et la règle quand elle écrit la valeur.
//! * Vérifié avant de rendre la main : les classes touchées se rechargent, et
//!   chaque ligne appliquée doit ressortir « déjà à jour », sinon tous les
//!   fichiers reviennent comme avant.
//!
//! Les règles se modifient ligne par ligne, jamais réécrites : elles portent
//! des commentaires et une mise en forme à garder.

use std::path::{Path, PathBuf};

use super::json_ordonne::Json;
use super::{lire_avec, normaliser, valeur, Champ, Etat, Ligne, Note, Valeur};
use crate::solve::ruleset_for;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

/// Ce qu'une application a fait.
#[derive(Debug)]
pub struct Application {
    pub appliquees: Vec<Ligne>,
    /// Les lignes à appliquer qui ne l'ont pas été, et pourquoi.
    pub laissees: Vec<(Ligne, String)>,
    pub fichiers: Vec<PathBuf>,
}

/// Les règles d'une classe telles que les fichiers d'un dossier `data` les
/// donnent, relevé fusionné, comme `load_ruleset` le fait de la donnée
/// compilée.
pub fn charger_depuis(dossier: &Path, classe: u32) -> Result<Ruleset, String> {
    let (nom, breed) = ruleset_for(classe).ok_or_else(|| format!("classe {classe} inconnue"))?;
    let lire = |chemin: PathBuf| std::fs::read_to_string(&chemin).map_err(|e| format!("{}: {e}", chemin.display()));
    let mut regles = Ruleset::from_yaml(&lire(dossier.join(format!("rulesets/{nom}.yaml")))?).map_err(|e| e.to_string())?;
    let releve = Snapshot::from_json(&lire(dossier.join(format!("snapshots/breed-{breed}.json")))?)?;
    let fusion = regles.merge_snapshot(&releve);
    if !fusion.conflicts.is_empty() {
        return Err(format!("{nom} : règles et relevé en désaccord : {:?}", fusion.conflicts));
    }
    Ok(regles)
}

/// Une note de bêta : son forum ou son titre le disent.
pub fn est_une_beta(note: &Note) -> bool {
    note.forum.starts_with("Beta") || normaliser(&note.titre).contains("beta")
}

/// Les fichiers d'une classe, chargés une fois et gardés tels qu'ils étaient.
struct Fichiers {
    classe: u32,
    regles: Ruleset,
    chemin_yaml: PathBuf,
    yaml: Vec<String>,
    yaml_origine: String,
    chemin_json: PathBuf,
    json: Json,
    json_origine: String,
}

impl Fichiers {
    fn ouvrir(dossier: &Path, classe: u32) -> Result<Fichiers, String> {
        let (nom, breed) = ruleset_for(classe).ok_or_else(|| format!("classe {classe} inconnue"))?;
        let chemin_yaml = dossier.join(format!("rulesets/{nom}.yaml"));
        let chemin_json = dossier.join(format!("snapshots/breed-{breed}.json"));
        let yaml_origine = std::fs::read_to_string(&chemin_yaml).map_err(|e| e.to_string())?;
        let json_origine = std::fs::read_to_string(&chemin_json).map_err(|e| e.to_string())?;
        Ok(Fichiers {
            classe,
            regles: charger_depuis(dossier, classe)?,
            chemin_yaml,
            yaml: yaml_origine.split('\n').map(str::to_string).collect(),
            yaml_origine,
            chemin_json,
            json: Json::lire(&json_origine)?,
            json_origine,
        })
    }

    fn yaml_texte(&self) -> String {
        self.yaml.join("\n")
    }
}

/// Applique les lignes « à appliquer » d'une note de sortie aux fichiers du
/// dossier `data`, puis vérifie. Les autres lignes ne sont pas touchées.
pub fn appliquer(note: &Note, dossier: &Path, date: &str) -> Result<Application, String> {
    if est_une_beta(note) {
        return Err("une note de bêta ne s'applique jamais : passez-la à blanc".into());
    }
    let charger = |c: u32| charger_depuis(dossier, c).ok();
    let lignes = lire_avec(note, &charger);
    let mut fichiers: Vec<Fichiers> = Vec::new();
    let mut appliquees = Vec::new();
    let mut laissees = Vec::new();
    for l in lignes.iter().filter(|l| l.etat == Etat::AAppliquer) {
        if !fichiers.iter().any(|f| f.classe == l.classe) {
            fichiers.push(Fichiers::ouvrir(dossier, l.classe)?);
        }
        let Some(f) = fichiers.iter_mut().find(|f| f.classe == l.classe) else {
            continue;
        };
        // Une ligne qui vise plusieurs sorts passe en entier ou pas du tout.
        let (yaml, json) = (f.yaml.clone(), f.json.clone());
        let resultat = l.sorts.iter().try_for_each(|s| appliquer_au_sort(f, s, l, &lignes));
        match resultat {
            Ok(()) => appliquees.push(l.clone()),
            Err(raison) => {
                f.yaml = yaml;
                f.json = json;
                laissees.push((l.clone(), raison));
            }
        }
    }
    if appliquees.is_empty() {
        return Ok(Application { appliquees, laissees, fichiers: Vec::new() });
    }

    // L'écriture, puis la vérification, et tout revient si elle échoue.
    let chemin_version = dossier.join("version.json");
    let version_origine = std::fs::read_to_string(&chemin_version).map_err(|e| e.to_string())?;
    let mut ecrits: Vec<(PathBuf, String)> = Vec::new();
    let ecrire = |chemin: &Path, texte: &str, ecrits: &mut Vec<(PathBuf, String)>, origine: &str| {
        if texte == origine {
            return Ok(());
        }
        std::fs::write(chemin, texte).map_err(|e| format!("{}: {e}", chemin.display()))?;
        ecrits.push((chemin.to_path_buf(), origine.to_string()));
        Ok::<(), String>(())
    };
    let mut resultat: Result<(), String> = Ok(());
    for f in &fichiers {
        resultat = resultat
            .and_then(|()| ecrire(&f.chemin_yaml, &f.yaml_texte(), &mut ecrits, &f.yaml_origine))
            .and_then(|()| ecrire(&f.chemin_json, &f.json.ecrire(), &mut ecrits, &f.json_origine));
    }
    // Les sorts que la note change, par `dofusdb_id` : les comparaisons
    // avec une source restée à la version d'avant les écartent.
    let mut changes: Vec<u32> = appliquees
        .iter()
        .flat_map(|l| {
            let regles = fichiers.iter().find(|f| f.classe == l.classe).map(|f| &f.regles);
            l.sorts.iter().filter_map(move |s| regles?.spells.iter().find(|x| &x.id == s)?.dofusdb_id)
        })
        .collect();
    changes.sort_unstable();
    changes.dedup();
    resultat = resultat.and_then(|()| {
        let version = noter_la_version(&version_origine, note, appliquees.len(), &changes, date)?;
        ecrire(&chemin_version, &version, &mut ecrits, &version_origine)
    });
    resultat = resultat.and_then(|()| verifier(note, dossier, &appliquees));
    if let Err(e) = resultat {
        for (chemin, origine) in &ecrits {
            let _ = std::fs::write(chemin, origine);
        }
        return Err(format!("{e} ; les fichiers sont revenus comme avant"));
    }
    Ok(Application { appliquees, laissees, fichiers: ecrits.into_iter().map(|(c, _)| c).collect() })
}

/// Les classes touchées se rechargent sans désaccord, et chaque ligne
/// appliquée ressort déjà à jour.
fn verifier(note: &Note, dossier: &Path, appliquees: &[Ligne]) -> Result<(), String> {
    let mut classes: Vec<u32> = appliquees.iter().map(|l| l.classe).collect();
    classes.sort_unstable();
    classes.dedup();
    for c in classes {
        charger_depuis(dossier, c)?;
    }
    let relues = lire_avec(note, &|c| charger_depuis(dossier, c).ok());
    for a in appliquees {
        let pareille = relues.iter().find(|r| r.classe == a.classe && r.sorts == a.sorts && r.texte == a.texte);
        if !pareille.is_some_and(|r| r.etat == Etat::DejaAJour) {
            return Err(format!("après écriture, « {} » ne ressort pas à jour", a.texte));
        }
    }
    Ok(())
}

/// `version.json` : la version que la note porte dans son titre, et la note
/// ajoutée à la liste des notes appliquées, avec les sorts qu'elle change.
fn noter_la_version(texte: &str, note: &Note, lignes: usize, sorts: &[u32], date: &str) -> Result<String, String> {
    let mut v = Json::lire(texte)?;
    let version = note
        .titre
        .split(|c: char| !(c.is_ascii_digit() || c == '.'))
        .find(|m| m.contains('.') && m.split('.').all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())));
    if let (Some(version), Some(Json::Texte(actuelle))) = (version, v.champ_mut("version_du_jeu")) {
        *actuelle = version.to_string();
    }
    let entree = Json::Objet(vec![
        ("id".into(), Json::Texte(note.id.clone())),
        ("titre".into(), Json::Texte(note.titre.clone())),
        ("appliquee_le".into(), Json::Texte(date.to_string())),
        ("lignes".into(), Json::Nombre((lignes as u64).into())),
        ("sorts".into(), Json::Liste(sorts.iter().map(|s| Json::Nombre(u64::from(*s).into())).collect())),
    ]);
    v.champ_mut("patch_notes")
        .and_then(Json::liste_mut)
        .ok_or("version.json sans liste patch_notes")?
        .push(entree);
    Ok(v.ecrire())
}

/// Les valeurs d'une note grade par grade : « 4 / 5 / 6 » en donne trois, une
/// valeur seule en donne une.
fn par_grade(champ: Champ, texte: &str) -> Vec<Valeur> {
    let morceaux: Vec<Option<Valeur>> = texte.split('/').map(|m| valeur(champ, m)).collect();
    if morceaux.len() > 1 && morceaux.iter().all(Option::is_some) {
        morceaux.into_iter().flatten().collect()
    } else {
        valeur(champ, texte).into_iter().collect()
    }
}

/// Les grades à changer et leurs deux valeurs : chacun le sien quand la note
/// chiffre autant de grades que le relevé en a, tous avec la même valeur
/// quand elle n'en donne qu'une, et sinon le plus haut seulement.
fn grades(avant: &[Valeur], apres: &[Valeur], niveaux: usize) -> Vec<(usize, Valeur, Valeur)> {
    match (avant.len(), apres.len()) {
        (a, b) if a == niveaux && b == niveaux => (0..niveaux).map(|i| (i, avant[i], apres[i])).collect(),
        (1, 1) => (0..niveaux).map(|i| (i, avant[0], apres[0])).collect(),
        (a, b) if a > 0 && b > 0 && niveaux > 0 => vec![(niveaux - 1, avant[a - 1], apres[b - 1])],
        _ => Vec::new(),
    }
}

/// Une ligne appliquée à un sort : son relevé, puis sa règle écrite à la main.
fn appliquer_au_sort(f: &mut Fichiers, sort: &str, l: &Ligne, toutes: &[Ligne]) -> Result<(), String> {
    let champ = l.champ.ok_or("champ inconnu")?;
    let dofusdb = f.regles.spells.iter().find(|s| s.id == sort).and_then(|s| s.dofusdb_id);
    let avant_texte = l.avant.as_deref().ok_or("sans avant")?;
    let apres_texte = l.apres.as_deref().ok_or("sans après")?;
    let (avant, apres) = (par_grade(champ, avant_texte), par_grade(champ, apres_texte));
    let (Some(avant_haut), Some(apres_haut)) = (avant.last().copied(), apres.last().copied()) else {
        return Err("valeurs illisibles".into());
    };
    if let Some(id) = dofusdb {
        appliquer_au_releve(&mut f.json, id, champ, &avant, &apres, l, toutes, sort)?;
    }
    appliquer_aux_regles(&mut f.yaml, sort, champ, avant_haut, apres_haut)
}

/// La clé du relevé, au niveau d'un grade, pour un champ qui s'y écrit d'un
/// seul nombre.
fn cle_du_releve(champ: Champ) -> Option<&'static str> {
    Some(match champ {
        Champ::CoutPa => "ap_cost",
        Champ::LancersParTour => "max_cast_per_turn",
        Champ::LancersParCible => "max_cast_per_target",
        Champ::Relance => "min_cast_interval",
        Champ::Critique => "base_crit_percent",
        _ => return None,
    })
}

fn nombre(v: Valeur) -> Option<i64> {
    match v {
        Valeur::Nombre(n) => Some(n),
        _ => None,
    }
}

#[allow(clippy::too_many_arguments)]
fn appliquer_au_releve(
    json: &mut Json,
    id: u32,
    champ: Champ,
    avant: &[Valeur],
    apres: &[Valeur],
    l: &Ligne,
    toutes: &[Ligne],
    sort: &str,
) -> Result<(), String> {
    let sorts = json.champ_mut("spells").and_then(Json::liste_mut).ok_or("relevé sans sorts")?;
    let s = sorts
        .iter_mut()
        .find(|s| s.champ("id").and_then(Json::entier) == Some(i64::from(id)))
        .ok_or_else(|| format!("sort {id} absent du relevé"))?;
    let niveaux = s.champ_mut("levels").and_then(Json::liste_mut).ok_or("sort sans grades")?;
    let n = niveaux.len();
    // Le grade le plus haut doit changer : c'est celui que la note a comparé.
    let mut haut_change = false;
    let mut changer = |i: usize, niveau: &mut Json, a: Valeur, b: Valeur| -> Result<(), String> {
        let fait = changer_un_grade(niveau, champ, a, b)?;
        if i + 1 == n {
            haut_change |= fait;
        }
        Ok(())
    };
    if champ == Champ::Degats {
        // Les dégâts par niveau : les lignes du même sort, rangées par niveau,
        // vont chacune à son grade quand la note en chiffre autant.
        let mut par_niveau: Vec<&Ligne> = toutes
            .iter()
            .filter(|x| x.classe == l.classe && x.sorts.iter().any(|s| s == sort) && x.champ == Some(Champ::Degats))
            .filter(|x| x.niveau.is_some())
            .collect();
        par_niveau.sort_by_key(|x| x.niveau);
        let paires: Vec<(Valeur, Valeur)> = if l.niveau.is_some() && par_niveau.len() == n {
            par_niveau
                .iter()
                .filter_map(|x| Some((valeur(champ, x.avant.as_deref()?)?, valeur(champ, x.apres.as_deref()?)?)))
                .collect()
        } else {
            Vec::new()
        };
        if paires.len() == n {
            for (i, (niveau, (a, b))) in niveaux.iter_mut().zip(paires).enumerate() {
                changer(i, niveau, a, b)?;
            }
        } else {
            let (a, b) = (avant[avant.len() - 1], apres[apres.len() - 1]);
            changer(n - 1, &mut niveaux[n - 1], a, b)?;
        }
    } else {
        for (i, a, b) in grades(avant, apres, n) {
            changer(i, &mut niveaux[i], a, b)?;
        }
    }
    if haut_change {
        Ok(())
    } else {
        Err("le relevé ne porte pas l'avant au grade le plus haut".into())
    }
}

/// Un grade du relevé : la valeur passe de `a` à `b` s'il porte `a`. Rend
/// faux quand il ne la porte pas, sans rien changer.
fn changer_un_grade(niveau: &mut Json, champ: Champ, a: Valeur, b: Valeur) -> Result<bool, String> {
    let entier = |n: i64| Json::Nombre(n.into());
    if let Some(cle) = cle_du_releve(champ) {
        let (Some(a), Some(b)) = (nombre(a), nombre(b)) else {
            return Err("valeur non numérique".into());
        };
        let v = niveau.champ_mut(cle).ok_or_else(|| format!("grade sans {cle}"))?;
        if v.entier() != Some(a) {
            return Ok(false);
        }
        *v = entier(b);
        return Ok(true);
    }
    match champ {
        Champ::Portee | Champ::PorteeMin | Champ::PorteeMax => {
            let portee = niveau.champ_mut("range").and_then(Json::liste_mut).ok_or("grade sans portée")?;
            let (Some(min), Some(max)) = (portee.first().and_then(Json::entier), portee.get(1).and_then(Json::entier)) else {
                return Err("portée illisible".into());
            };
            let (porte, nouvelle) = match (champ, a, b) {
                (Champ::Portee, Valeur::Paire(a1, a2), Valeur::Paire(b1, b2)) => ((min, max) == (a1, a2), (b1, b2)),
                (Champ::PorteeMin, Valeur::Nombre(a), Valeur::Nombre(b)) => (min == a, (b, max)),
                (Champ::PorteeMax, Valeur::Nombre(a), Valeur::Nombre(b)) => (max == a, (min, b)),
                _ => return Err("portée illisible".into()),
            };
            if !porte {
                return Ok(false);
            }
            portee[0] = entier(nouvelle.0);
            portee[1] = entier(nouvelle.1);
            Ok(true)
        }
        Champ::LancerEnLigne | Champ::LancerEnDiagonale | Champ::LancerEnLigneEtDiagonale | Champ::LigneDeVue
        | Champ::PorteeModifiable => {
            let (Some(a), Some(b)) = (nombre(a), nombre(b)) else {
                return Err("valeur non booléenne".into());
            };
            let cles: &[&str] = match champ {
                Champ::LancerEnLigne => &["in_line"],
                Champ::LancerEnDiagonale => &["in_diagonal"],
                Champ::LancerEnLigneEtDiagonale => &["in_line", "in_diagonal"],
                Champ::LigneDeVue => &["needs_line_of_sight"],
                _ => &["range_boostable"],
            };
            let lancer = niveau.champ_mut("cast").ok_or("grade sans règles de lancer")?;
            if !cles.iter().all(|c| lancer.champ(c).and_then(Json::booleen) == Some(a == 1)) {
                return Ok(false);
            }
            for c in cles {
                if let Some(v) = lancer.champ_mut(c) {
                    *v = Json::Bool(b == 1);
                }
            }
            Ok(true)
        }
        Champ::Degats => {
            let (Valeur::Degats(na, ca), Valeur::Degats(nb, cb)) = (a, b) else {
                return Err("dégâts illisibles".into());
            };
            let mut fait = false;
            for (cle, ancienne, nouvelle) in [("normal_lines", Some(na), Some(nb)), ("critical_lines", ca, cb)] {
                let (Some(ancienne), Some(nouvelle)) = (ancienne, nouvelle) else {
                    continue;
                };
                let Some(lignes) = niveau.champ_mut(cle).and_then(Json::liste_mut) else {
                    continue;
                };
                for ligne in lignes.iter_mut() {
                    // Un vol de vie frappe comme des dégâts.
                    if !matches!(ligne.champ("kind").and_then(Json::texte), Some("damage" | "life_steal")) {
                        continue;
                    }
                    let Some(r) = ligne.champ_mut("range").and_then(Json::liste_mut) else {
                        continue;
                    };
                    if r.first().and_then(Json::entier) == Some(ancienne.0) && r.get(1).and_then(Json::entier) == Some(ancienne.1)
                    {
                        r[0] = entier(nouvelle.0);
                        r[1] = entier(nouvelle.1);
                        fait |= cle == "normal_lines";
                    }
                }
            }
            Ok(fait)
        }
        // Le bonus de dégâts de base n'est écrit que dans les règles.
        Champ::BonusDegatsBase => Ok(true),
        _ => Err("champ que le relevé ne porte pas".into()),
    }
}

/// Le bloc d'un sort dans les règles : sa première ligne, la fin exclue, et le
/// retrait de ses champs.
fn bloc_du_sort(yaml: &[String], sort: &str) -> Option<(usize, usize, usize)> {
    let entete = format!("- id: {sort}");
    let debut = yaml.iter().position(|l| l.trim() == entete)?;
    let tiret = yaml[debut].len() - yaml[debut].trim_start().len();
    let fin = yaml[debut + 1..]
        .iter()
        .position(|l| !l.trim().is_empty() && l.len() - l.trim_start().len() <= tiret)
        .map_or(yaml.len(), |p| debut + 1 + p);
    Some((debut, fin, tiret + 2))
}

/// Le nombre qui suit `cle` dans une ligne, et sa place.
fn nombre_apres(ligne: &str, cle: &str) -> Option<(usize, usize, i64)> {
    let mut depuis = 0;
    while let Some(p) = ligne[depuis..].find(cle) {
        let i = depuis + p;
        depuis = i + cle.len();
        // `amount:` ne doit pas prendre `critical_amount:`.
        if i > 0 && ligne.as_bytes()[i - 1].is_ascii_alphanumeric() || i > 0 && ligne.as_bytes()[i - 1] == b'_' {
            continue;
        }
        let reste = &ligne[depuis..];
        let blanc = reste.len() - reste.trim_start().len();
        let chiffres: String = reste.trim_start().chars().take_while(char::is_ascii_digit).collect();
        if chiffres.is_empty() {
            continue;
        }
        let debut = depuis + blanc;
        return Some((debut, debut + chiffres.len(), chiffres.parse().ok()?));
    }
    None
}

/// Les deux nombres d'une paire « [a, b] » qui suit `cle`.
fn paire_apres(ligne: &str, cle: &str) -> Option<(usize, usize, (i64, i64))> {
    let i = ligne.find(cle)? + cle.len();
    let ouvre = i + ligne[i..].find('[')?;
    let ferme = ouvre + ligne[ouvre..].find(']')?;
    let n: Vec<i64> = ligne[ouvre + 1..ferme].split(',').map(|m| m.trim().parse().ok()).collect::<Option<_>>()?;
    match n.as_slice() {
        [a, b] => Some((ouvre, ferme + 1, (*a, *b))),
        _ => None,
    }
}

/// La règle écrite à la main d'un sort : la valeur passe de l'avant à l'après
/// là où elle est écrite. Absente, ou laissée inconnue, le relevé suffit.
fn appliquer_aux_regles(yaml: &mut [String], sort: &str, champ: Champ, a: Valeur, b: Valeur) -> Result<(), String> {
    let Some((debut, fin, retrait)) = bloc_du_sort(yaml, sort) else {
        return Ok(());
    };
    let au_retrait = |l: &str| l.len() - l.trim_start().len() == retrait;
    let remplacer_nombre = |yaml: &mut [String], i: usize, cle: &str, a: i64, b: i64| -> Result<bool, String> {
        let Some((d, f, v)) = nombre_apres(&yaml[i], cle) else {
            return Ok(false);
        };
        if v == b {
            return Ok(true);
        }
        if v != a {
            return Err(format!("règle écrite à la main : {cle} {v}, la note dit {a}"));
        }
        yaml[i].replace_range(d..f, &b.to_string());
        Ok(true)
    };
    let simple = |cle: &str| -> Option<usize> {
        (debut + 1..fin).find(|&i| au_retrait(&yaml[i]) && yaml[i].trim_start().starts_with(cle))
    };
    match champ {
        Champ::CoutPa => {
            let (Some(a), Some(b)) = (nombre(a), nombre(b)) else {
                return Err("coût illisible".into());
            };
            let Some(i) = simple("ap_cost:") else {
                return Ok(());
            };
            if !remplacer_nombre(yaml, i, "base:", a, b)? {
                // `ap_cost:` sur sa ligne, `base:` sur une suivante.
                if let Some(j) = (i + 1..fin).take_while(|&j| !au_retrait(&yaml[j])).find(|&j| yaml[j].trim_start().starts_with("base:")) {
                    remplacer_nombre(yaml, j, "base:", a, b)?;
                }
            }
            Ok(())
        }
        Champ::LancersParTour | Champ::LancersParCible | Champ::Relance | Champ::Critique => {
            let (Some(a), Some(b)) = (nombre(a), nombre(b)) else {
                return Err("valeur illisible".into());
            };
            let (cle_ligne, cle_nombre) = match champ {
                Champ::LancersParTour => ("casts_per_turn:", "casts_per_turn:"),
                Champ::LancersParCible => ("casts_per_target:", "casts_per_target:"),
                Champ::Relance => ("cooldown_turns:", "cooldown_turns:"),
                _ => ("crit:", "base_rate:"),
            };
            if let Some(i) = simple(cle_ligne) {
                remplacer_nombre(yaml, i, cle_nombre, a, b)?;
            }
            Ok(())
        }
        Champ::Degats => {
            let (Valeur::Degats(na, ca), Valeur::Degats(nb, cb)) = (a, b) else {
                return Err("dégâts illisibles".into());
            };
            for (cle, ancienne, nouvelle) in [("normal:", Some(na), Some(nb)), ("critical:", ca, cb)] {
                let (Some(ancienne), Some(nouvelle)) = (ancienne, nouvelle) else {
                    continue;
                };
                for ligne in yaml.iter_mut().take(fin).skip(debut + 1) {
                    if !ligne.trim_start().starts_with(cle) {
                        continue;
                    }
                    let Some((d, f, v)) = paire_apres(ligne, cle) else {
                        continue;
                    };
                    if v == nouvelle {
                        continue;
                    }
                    if v != ancienne {
                        return Err(format!("règle écrite à la main : {cle} {v:?}, la note dit {ancienne:?}"));
                    }
                    ligne.replace_range(d..f, &format!("[{}, {}]", nouvelle.0, nouvelle.1));
                }
            }
            Ok(())
        }
        Champ::BonusDegatsBase => {
            let (Some(a), Some(b)) = (nombre(a), nombre(b)) else {
                return Err("bonus illisible".into());
            };
            let Some(i) = (debut + 1..fin).find(|&i| yaml[i].contains("base_bonus:")) else {
                return Err("aucun bonus de dégâts de base dans la règle".into());
            };
            let retrait_bonus = yaml[i].len() - yaml[i].trim_start().len();
            let zone: Vec<usize> = std::iter::once(i)
                .chain((i + 1..fin).take_while(|&j| yaml[j].trim().is_empty() || yaml[j].len() - yaml[j].trim_start().len() > retrait_bonus))
                .collect();
            let mut vus = 0;
            for j in zone {
                if nombre_apres(&yaml[j], "amount:").is_some() {
                    remplacer_nombre(yaml, j, "amount:", a, b)?;
                    vus += 1;
                }
            }
            if vus == 0 {
                return Err("bonus de dégâts de base sans montant écrit".into());
            }
            Ok(())
        }
        // La portée et les règles de lancer ne viennent que du relevé.
        _ => Ok(()),
    }
}

/// La date du jour, AAAA-MM-JJ, en temps universel.
pub fn aujourd_hui() -> String {
    let secondes = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    // Les jours depuis 1970 en date civile (algorithme de Howard Hinnant).
    let z = (secondes / 86_400) as i64 + 719_468;
    let ere = z.div_euclid(146_097);
    let jour_ere = z - ere * 146_097;
    let annee_ere = (jour_ere - jour_ere / 1460 + jour_ere / 36_524 - jour_ere / 146_096) / 365;
    let jour_annee = jour_ere - (365 * annee_ere + annee_ere / 4 - annee_ere / 100);
    let mp = (5 * jour_annee + 2) / 153;
    let jour = jour_annee - (153 * mp + 2) / 5 + 1;
    let mois = if mp < 10 { mp + 3 } else { mp - 9 };
    let annee = annee_ere + ere * 400 + i64::from(mois <= 2);
    format!("{annee:04}-{mois:02}-{jour:02}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::notes::Noeud;
    use dofus_ruleset::Maybe;

    /// Un dossier `data` jetable, copie de celui du dépôt pour le Crâ.
    struct Bac(PathBuf);

    impl Bac {
        fn nouveau(nom: &str) -> Bac {
            let racine = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data");
            let bac = std::env::temp_dir().join(format!("krozties-note-{nom}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&bac);
            std::fs::create_dir_all(bac.join("rulesets")).unwrap();
            std::fs::create_dir_all(bac.join("snapshots")).unwrap();
            for f in ["rulesets/cra.yaml", "snapshots/breed-9.json", "version.json"] {
                std::fs::copy(racine.join(f), bac.join(f)).unwrap();
            }
            Bac(bac)
        }
    }

    impl Drop for Bac {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn noeud(balise: &str, profondeur: u8, texte: &str) -> Noeud {
        Noeud { balise: balise.into(), profondeur, texte: texte.into() }
    }

    fn note_de_sortie(noeuds: Vec<Noeud>) -> Note {
        Note {
            id: "1".into(),
            titre: "Patch notes 9.9 du 08/10/2026".into(),
            forum: "Patch notes".into(),
            url: String::new(),
            noeuds,
        }
    }

    /// ⚠️ UNE LIGNE APPLIQUÉE CHANGE LE RELEVÉ ET LA RÈGLE, et rien d'autre :
    /// le coût en PA de la Flèche de Recul, que la règle écrit à la main, sa
    /// portée maximale, que seul le relevé porte, et ses dégâts. La classe se
    /// recharge, la note ressort à jour, et la version la note.
    #[test]
    fn une_note_de_sortie_s_applique_au_releve_et_aux_regles() {
        let bac = Bac::nouveau("sortie");
        let r = charger_depuis(&bac.0, 9).unwrap();
        let s = r.spells.iter().find(|s| s.id == "fleche_de_recul").unwrap();
        let (pa, max) = (s.ap_cost.base, s.range.unwrap().1);
        let l = s.lines.iter().find(|l| matches!(l.normal, Maybe::Known(_))).unwrap();
        let (Maybe::Known((a, b)), Maybe::Known((c, d))) = (l.normal, l.critical) else { unreachable!() };
        let yaml_avant = std::fs::read_to_string(bac.0.join("rulesets/cra.yaml")).unwrap();
        let json_avant = std::fs::read_to_string(bac.0.join("snapshots/breed-9.json")).unwrap();
        let note = note_de_sortie(vec![
            noeud("h3", 0, "Crâ"),
            noeud("li", 1, &s.name.fr),
            noeud("li", 2, &format!("Coût en PA : {pa} → {}", pa + 1)),
            noeud("li", 2, &format!("Portée maximale : {max} → {}", max + 1)),
            noeud("li", 2, &format!("Dommages : {a} à {b} ({c} à {d}) → {} à {} ({} à {})", a + 1, b + 1, c + 1, d + 1)),
            noeud("li", 2, "Une phrase."),
        ]);
        let fait = appliquer(&note, &bac.0, "2026-10-08").unwrap();
        assert_eq!(fait.appliquees.len(), 3, "{:?}", fait.laissees);
        let r = charger_depuis(&bac.0, 9).unwrap();
        let s = r.spells.iter().find(|s| s.id == "fleche_de_recul").unwrap();
        assert_eq!((s.ap_cost.base, s.range.unwrap().1), (pa + 1, max + 1));
        let l = s.lines.iter().find(|l| matches!(l.normal, Maybe::Known(_))).unwrap();
        assert_eq!(l.normal, Maybe::Known((a + 1, b + 1)));
        // Seules les lignes visées ont bougé.
        let yaml_apres = std::fs::read_to_string(bac.0.join("rulesets/cra.yaml")).unwrap();
        let changees = yaml_avant.lines().zip(yaml_apres.lines()).filter(|(x, y)| x != y).count();
        assert!(changees >= 1 && changees <= 3, "{changees} lignes de règles changées");
        assert_eq!(yaml_avant.lines().count(), yaml_apres.lines().count());
        let json_apres = std::fs::read_to_string(bac.0.join("snapshots/breed-9.json")).unwrap();
        let changees = json_avant.lines().zip(json_apres.lines()).filter(|(x, y)| x != y).count();
        assert!(changees >= 3, "{changees} lignes du relevé changées");
        assert_eq!(json_avant.lines().count(), json_apres.lines().count());
        let version = std::fs::read_to_string(bac.0.join("version.json")).unwrap();
        assert!(version.contains("\"version_du_jeu\": \"9.9\""), "{version}");
        assert!(version.contains("\"appliquee_le\": \"2026-10-08\""), "{version}");
        // Rejouée, la note n'a plus rien à appliquer.
        let deux = appliquer(&note, &bac.0, "2026-10-08").unwrap();
        assert!(deux.appliquees.is_empty() && deux.fichiers.is_empty());
    }

    /// Un vol de vie est une ligne de dégâts comme une autre : la note change
    /// ses chiffres. Œil pour Œil du Crâ, laissé à la sortie de la 3.7.
    #[test]
    fn un_vol_de_vie_s_applique() {
        let bac = Bac::nouveau("vol");
        let r = charger_depuis(&bac.0, 9).unwrap();
        let s = r.spells.iter().find(|s| s.id == "il_pour_il").unwrap();
        let l = s.lines.iter().find(|l| matches!(l.normal, Maybe::Known(_))).unwrap();
        let (Maybe::Known((a, b)), Maybe::Known((c, d))) = (l.normal, l.critical) else { unreachable!() };
        let note = note_de_sortie(vec![
            noeud("h3", 0, "Crâ"),
            noeud("li", 1, &s.name.fr),
            noeud("li", 2, &format!("Dommages : {a} à {b} ({c} à {d}) → {} à {} ({} à {})", a - 1, b - 1, c - 1, d - 1)),
        ]);
        let fait = appliquer(&note, &bac.0, "2026-10-06").unwrap();
        assert_eq!(fait.appliquees.len(), 1, "{:?}", fait.laissees);
        let r = charger_depuis(&bac.0, 9).unwrap();
        let s = r.spells.iter().find(|s| s.id == "il_pour_il").unwrap();
        let l = s.lines.iter().find(|l| matches!(l.normal, Maybe::Known(_))).unwrap();
        assert_eq!((l.normal, l.critical), (Maybe::Known((a - 1, b - 1)), Maybe::Known((c - 1, d - 1))));
    }

    /// ⚠️ JAMAIS UNE NOTE DE BÊTA, et rien n'est écrit.
    #[test]
    fn une_note_de_beta_ne_s_applique_pas() {
        let bac = Bac::nouveau("beta");
        let mut note = note_de_sortie(vec![noeud("h3", 0, "Crâ")]);
        note.forum = "Beta (Patch notes bêta 3.7)".into();
        assert!(appliquer(&note, &bac.0, "2026-10-08").is_err());
        let mut note = note_de_sortie(vec![noeud("h3", 0, "Crâ")]);
        note.titre = "Patch Notes Bêta 3.7 - 17/09/2026".into();
        assert!(appliquer(&note, &bac.0, "2026-10-08").is_err());
    }

    /// Une règle écrite à la main qui porte autre chose que l'avant arrête la
    /// ligne, et la laisse au rapport : rien ne change.
    #[test]
    fn une_regle_ecrite_autrement_arrete_la_ligne() {
        let bac = Bac::nouveau("regle");
        let r = charger_depuis(&bac.0, 9).unwrap();
        let s = r.spells.iter().find(|s| s.id == "fleche_de_recul").unwrap();
        let tours = s.casts_per_turn;
        // Le relevé porte l'avant, la règle autre chose : on fausse la règle.
        let chemin = bac.0.join("rulesets/cra.yaml");
        let yaml = std::fs::read_to_string(&chemin).unwrap();
        let mut lignes: Vec<String> = yaml.split('\n').map(str::to_string).collect();
        let (debut, fin, _) = bloc_du_sort(&lignes, "fleche_de_recul").unwrap();
        let i = (debut..fin).find(|&i| lignes[i].trim_start().starts_with("casts_per_turn:")).unwrap();
        let mut fichier = Fichiers::ouvrir(&bac.0, 9).unwrap();
        lignes[i] = lignes[i].replace(&tours.to_string(), &(tours + 5).to_string());
        fichier.yaml = lignes;
        let ligne = Ligne {
            classe: 9,
            sorts: vec!["fleche_de_recul".into()],
            texte: String::new(),
            champ: Some(Champ::LancersParTour),
            niveau: None,
            avant: Some(tours.to_string()),
            apres: Some((tours + 1).to_string()),
            etat: Etat::AAppliquer,
        };
        let refus = appliquer_au_sort(&mut fichier, "fleche_de_recul", &ligne, &[]);
        assert!(refus.is_err_and(|e| e.contains("règle écrite à la main")));
    }

    #[test]
    fn la_date_du_jour_a_la_forme_attendue() {
        let d = aujourd_hui();
        assert_eq!(d.len(), 10);
        assert!(d.starts_with("20"), "{d}");
    }
}
