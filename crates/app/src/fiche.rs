//! La fiche du build, calculée depuis les effets d'objets d'un build DofusBook
//! importé, et non depuis notre catalogue.
//!
//! Chaque règle est vérifiée contre les totaux et le détail que leur page
//! affiche, sur deux builds réels : le test en bas de fichier les rejoue au point
//! près. Leur détail donne la règle en toutes lettres, « Bonus sagesse 41 » pour
//! 415 de Sagesse, « Personnage 1050 » au niveau 200.

use std::collections::BTreeMap;

use dofus_build::Boost;

/// Les caractéristiques qui ont un capital et un parchemin.
const CARACTERISTIQUES: [&str; 6] = ["vi", "sa", "fo", "in", "ch", "ag"];

/// Les dommages élémentaires, auxquels s'ajoutent les « Dommages » tout court.
const DOMMAGES_ELEMENTAIRES: [&str; 5] = ["dnf", "dtf", "dff", "def", "daf"];

/// Les statistiques qui reçoivent un dixième de la Sagesse.
const PAR_SAGESSE: [&str; 4] = ["epa", "epm", "rpa", "rpm"];

#[derive(Debug, Default)]
pub struct Fiche {
    /// Les totaux, sous les codes de DofusBook.
    pub stats: BTreeMap<String, f64>,
    /// La provenance de chaque total, ligne par ligne, dans l'ordre de leur
    /// page : capital, parchemin, personnage, forgemagie globale, objets par
    /// emplacement, panoplies, puis les bonus dérivés.
    pub details: BTreeMap<String, Vec<(String, f64)>>,
    /// Les statistiques qu'un sort de classe compté en bonus a modifiées. Le
    /// moteur lance lui-même ces sorts quand il sait les modéliser : une
    /// comparaison sur ces lignes signalerait un écart qui n'en est pas un.
    pub touchees_par_un_sort: Vec<String>,
}

impl Fiche {
    fn ajouter(&mut self, code: &str, libelle: &str, valeur: f64) {
        if valeur == 0.0 {
            // Leur détail omet les lignes nulles : un capital de Vitalité à zéro
            // n'apparaît pas.
            return;
        }
        *self.stats.entry(code.to_string()).or_default() += valeur;
        self.details
            .entry(code.to_string())
            .or_default()
            .push((libelle.to_string(), valeur));
    }

    fn total(&self, code: &str) -> f64 {
        self.stats.get(code).copied().unwrap_or(0.0)
    }

    /// Au format de leur magasin d'état : `{ stats, details }`.
    pub fn en_json(&self) -> serde_json::Value {
        let nombre = |v: f64| {
            if v.fract() == 0.0 {
                serde_json::json!(v as i64)
            } else {
                serde_json::json!((v * 10.0).round() / 10.0)
            }
        };
        serde_json::json!({
            "stats": self.stats.iter().map(|(k, v)| (k.clone(), nombre(*v)))
                .collect::<serde_json::Map<_, _>>(),
            "details": self.details.iter().map(|(k, lignes)| {
                (k.clone(), serde_json::Value::from(lignes.iter().map(|(l, v)| {
                    serde_json::json!({ "label": l, "value": nombre(*v) })
                }).collect::<Vec<_>>()))
            }).collect::<serde_json::Map<_, _>>(),
        })
    }
}

/// Un effet d'objet : la valeur retenue est la plus haute des deux bornes, y
/// compris pour un malus (de -301 à -400, la page compte -301 ; de 201 à 250,
/// 250).
fn valeur_d_effet(effet: &serde_json::Value) -> Option<(String, f64)> {
    let code = effet.get(0)?.as_str()?.to_string();
    let min = effet.get(1)?.as_f64()?;
    let max = effet.get(2)?.as_f64()?;
    Some((code, min.max(max)))
}

/// Calcule la fiche à partir des variables d'un build DofusBook. `bonus` sont
/// les bonus actifs du build, comptés au total sans en faire une ligne (des
/// « Dommages » à 20 au détail vide, venus de l'Harmonie de Pandala) : c'est
/// l'affichage qui nomme l'écart.
pub fn calculer(variables: &serde_json::Value, bonus: &[Boost]) -> Fiche {
    let mut f = Fiche::default();
    let niveau = variables["niveau"].as_f64().unwrap_or(200.0);
    let carac = &variables["carac"];

    for c in CARACTERISTIQUES {
        f.ajouter(c, "Capital", carac[format!("base_{c}")].as_f64().unwrap_or(0.0));
        f.ajouter(c, "Parchemin", carac[format!("scroll_{c}")].as_f64().unwrap_or(0.0));
    }

    // Ce que le personnage a sans rien porter. ⚠️ Les 7 PA ne sont vérifiés
    // qu'au niveau 200 ; le 6 sous le niveau 100 est la règle du jeu, pas une
    // mesure sur leur page.
    f.ajouter("pa", "Personnage", if niveau >= 100.0 { 7.0 } else { 6.0 });
    f.ajouter("pm", "Personnage", 3.0);
    f.ajouter("ic", "Personnage", 1.0);
    f.ajouter("pp", "Personnage", 100.0);
    f.ajouter("pd", "Personnage", 1000.0);

    // La forgemagie globale a sa propre ligne, avant les objets.
    if let Some(globale) = variables["fm_global"].as_object() {
        for (code, v) in globale {
            f.ajouter(code, "Forgemagie", v.as_f64().unwrap_or(0.0));
        }
    }

    // Les objets, dans l'ordre alphabétique des emplacements. La forgemagie
    // d'un emplacement s'ajoute à la ligne de l'objet (« Bouclier du Cycloïde
    // 8 » en Dommages critiques, ces 8 étant sa forgemagie).
    let objets: BTreeMap<i64, &serde_json::Value> = variables["objets"]
        .as_array()
        .map(|a| a.iter().filter_map(|o| Some((o["id"].as_i64()?, o))).collect())
        .unwrap_or_default();
    if let Some(emplacements) = variables["emplacements"].as_object() {
        let mut ordonnes: Vec<_> = emplacements.iter().collect();
        ordonnes.sort_by(|a, b| a.0.cmp(b.0));
        for (slot, id) in ordonnes {
            let Some(objet) = id.as_i64().and_then(|i| objets.get(&i)) else {
                continue;
            };
            let nom = objet["nom"].as_str().unwrap_or("?");
            let mut lignes: Vec<(String, f64)> = Vec::new();
            let mut cumuler = |code: String, v: f64| {
                match lignes.iter_mut().find(|(c, _)| *c == code) {
                    Some(l) => l.1 += v,
                    None => lignes.push((code, v)),
                }
            };
            for effet in objet["effets"].as_array().into_iter().flatten() {
                if let Some((code, v)) = valeur_d_effet(effet) {
                    cumuler(code, v);
                }
            }
            if let Some(fm) = variables["fm"][slot.as_str()].as_object() {
                for (code, v) in fm {
                    cumuler(code.clone(), v.as_f64().unwrap_or(0.0));
                }
            }
            for (code, v) in lignes {
                f.ajouter(&code, nom, v);
            }
        }
    }

    // Les panoplies : le palier qui correspond au nombre d'objets portés.
    for panoplie in variables["panoplies"].as_array().into_iter().flatten() {
        let nom = panoplie["nom"].as_str().unwrap_or("?");
        let portes = panoplie["nombre"].as_i64().unwrap_or(0);
        for effet in panoplie["effets"].as_array().into_iter().flatten() {
            let (Some(code), Some(palier), Some(v)) =
                (effet[0].as_str(), effet[1].as_i64(), effet[2].as_f64())
            else {
                continue;
            };
            if palier == portes {
                f.ajouter(code, nom, v);
            }
        }
    }

    // Les bonus actifs : au total, sans ligne. Les dommages finaux (`deg`) se
    // composent : 10 % et 5 % font 15,5 %, pas 15.
    let mut multiplicateur = 1.0;
    for b in bonus.iter().filter(|b| b.is_active()) {
        let v = f64::from(b.amount());
        if b.stat == "deg" {
            multiplicateur *= 1.0 + v / 100.0;
            continue;
        }
        *f.stats.entry(b.stat.clone()).or_default() += v;
        if b.class_id.is_some() {
            f.touchees_par_un_sort.push(b.stat.clone());
        }
    }
    if multiplicateur != 1.0 {
        f.stats.insert("deg".into(), (multiplicateur - 1.0) * 100.0);
    }

    // Les dérivées, en dernier : elles lisent les totaux.
    let dixieme = |v: f64| (v / 10.0).floor();
    let (fo, int, ch, ag, sa) =
        (f.total("fo"), f.total("in"), f.total("ch"), f.total("ag"), f.total("sa"));
    f.ajouter("ii", "Bonus caracs", fo + int + ch + ag);
    f.ajouter("pd", "Bonus force", fo * 5.0);
    f.ajouter("pp", "Bonus chance", dixieme(ch));
    f.ajouter("fu", "Bonus agilité", dixieme(ag));
    f.ajouter("ta", "Bonus agilité", dixieme(ag));
    for c in PAR_SAGESSE {
        f.ajouter(c, "Bonus sagesse", dixieme(sa));
    }

    // Les points de vie reprennent la Vitalité, lignes comprises, sous les
    // libellés de leur page.
    let mut vie = vec![("Personnage".to_string(), 50.0 + 5.0 * niveau)];
    for (libelle, v) in f.details.get("vi").cloned().unwrap_or_default() {
        let libelle = match libelle.as_str() {
            "Capital" => "Capital vitalité".to_string(),
            "Parchemin" => "Parchemin vitalité".to_string(),
            _ => libelle,
        };
        vie.push((libelle, v));
    }
    for (libelle, v) in vie {
        f.ajouter("pv", &libelle, v);
    }
    // Un bonus de Vitalité compte aussi en points de vie.
    let bonus_vitalite = f.total("vi")
        - f.details.get("vi").map_or(0.0, |l| l.iter().map(|(_, v)| v).sum());
    *f.stats.entry("pv".into()).or_default() += bonus_vitalite;

    // Les dommages élémentaires reçoivent les « Dommages », sans ligne.
    let dommages = f.total("dmg");
    for c in DOMMAGES_ELEMENTAIRES {
        *f.stats.entry(c.to_string()).or_default() += dommages;
    }

    f
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Les variables d'un premier build, telles que l'import les transmet.
    fn variables_build_1() -> serde_json::Value {
        serde_json::from_str(include_str!("../tests/fixtures/fiche-dofusbook-1.json"))
            .expect("premier jeu d'essai")
    }

    fn variables_build_2() -> serde_json::Value {
        serde_json::from_str(include_str!("../tests/fixtures/fiche-dofusbook-2.json"))
            .expect("second jeu d'essai")
    }

    fn bonus(v: &serde_json::Value) -> Vec<Boost> {
        serde_json::from_value(v["bonus"].clone()).expect("bonus")
    }

    /// Les totaux que DofusBook affiche, rejoués au point près : chaque nombre
    /// attendu est celui de la page, pas un calcul d'ici.
    fn verifier(fiche: &Fiche, attendus: &[(&str, f64)]) {
        let mut faux = Vec::new();
        for (code, attendu) in attendus {
            let calcule = fiche.stats.get(*code).copied().unwrap_or(0.0);
            if (calcule - attendu).abs() > 0.05 {
                faux.push(format!("{code} : {calcule} au lieu de {attendu}"));
            }
        }
        assert!(faux.is_empty(), "écarts avec la page de DofusBook : {faux:#?}");
    }

    #[test]
    fn le_calcul_rejoue_les_totaux_de_dofusbook_sur_le_build_1() {
        let v = variables_build_1();
        let fiche = calculer(&v["dofusbook"], &bonus(&v));
        verifier(&fiche, &[
            ("pv", 4105.0), ("pa", 12.0), ("pm", 6.0), ("po", 5.0), ("vi", 3055.0),
            ("sa", 415.0), ("fo", 1060.0), ("in", 300.0), ("ch", 100.0), ("ag", 750.0),
            ("pu", 110.0), ("ii", 2210.0), ("cc", 94.0), ("ic", 4.0), ("so", 0.0),
            ("pp", 185.0), ("fu", 80.0), ("ta", 93.0), ("dmg", 20.0), ("dnf", 104.0),
            ("dtf", 151.0), ("dff", 77.0), ("def", 45.0), ("daf", 124.0), ("dc", 163.0),
            ("dp", -21.0), ("deg", 15.5), ("rt", 30.0), ("re", 15.0), ("rnp", 28.0),
            ("rtp", 32.0), ("rfp", 24.0), ("rep", 10.0), ("rap", 31.0), ("rc", 30.0),
            ("epa", 30.0), ("epm", 46.0), ("rpa", 41.0), ("rpm", 58.0), ("pd", 6300.0),
        ]);
    }

    #[test]
    fn le_calcul_rejoue_les_totaux_de_dofusbook_sur_le_build_2() {
        let v = variables_build_2();
        let fiche = calculer(&v["dofusbook"], &bonus(&v));
        verifier(&fiche, &[
            ("pv", 3950.0), ("pa", 12.0), ("pm", 6.0), ("po", 5.0), ("vi", 2900.0),
            ("sa", 440.0), ("fo", 1015.0), ("in", 1015.0), ("ch", 330.0), ("ag", 350.0),
            ("pu", 320.0), ("ii", 3109.0), ("cc", 91.0), ("ic", 3.0), ("pp", 238.0),
            ("fu", 35.0), ("ta", 70.0), ("dmg", 20.0), ("dnf", 140.0), ("dtf", 145.0),
            ("dff", 145.0), ("def", 76.0), ("daf", 75.0), ("dc", 134.0), ("dp", 29.0),
            ("deg", 15.5), ("rt", 15.0), ("rf", 35.0), ("rnp", 15.0), ("rtp", 25.0),
            ("rfp", 33.0), ("rep", 28.0), ("rap", 42.0), ("rc", 55.0), ("rp", -15.0),
            ("epa", 44.0), ("epm", 38.0), ("rpa", 44.0), ("rpm", 66.0), ("pd", 6075.0),
        ]);
    }

    /// Le détail suit leur page : la forgemagie d'emplacement se fond dans la
    /// ligne de l'objet, la forgemagie globale a la sienne, un malus garde sa
    /// valeur la moins mauvaise.
    #[test]
    fn le_detail_suit_leur_page_ligne_a_ligne() {
        let v = variables_build_2();
        let fiche = calculer(&v["dofusbook"], &bonus(&v));
        let ligne = |code: &str, libelle: &str| {
            fiche.details[code].iter().find(|(l, _)| l == libelle).map(|(_, v)| *v)
        };
        assert_eq!(ligne("dc", "Bouclier du Cycloïde"), Some(8.0), "forgemagie d'emplacement");
        assert_eq!(ligne("dmg", "Forgemagie"), Some(20.0), "forgemagie globale");
        assert_eq!(ligne("ii", "Sangle Ouare"), Some(-301.0), "malus : la moins mauvaise borne");
        assert_eq!(ligne("pd", "Bonus force"), Some(5075.0));
        assert_eq!(fiche.details["fo"][0], ("Capital".to_string(), 265.0));
        // Le Prélude au Fer est un sort de Forgelance : son bonus compte au
        // total sans ligne, et la Puissance est marquée comme touchée par un sort.
        assert!(fiche.details["pu"].iter().all(|(l, _)| l != "Prélude au Fer"));
        assert!(fiche.touchees_par_un_sort.contains(&"pu".to_string()));
    }
}
