//! Les dégâts sur X cibles, et pourquoi le plafond de zone compte.
//!
//! Sans plafond, annoncer trois ennemis multiplierait tous les sorts par trois,
//! et le classement ne bougerait pas. Les sorts de zone montent vite, jusqu'à
//! la capacité de leur zone ; un mono-cible gagne un lancer puis n'avance plus,
//! car il porte deux limites, tant de lancers par tour et tant par cible.
//! L'Intimidation du Iop part trois fois par tour et deux fois sur la même
//! cible : deux lancers en duel, trois dès qu'il y a du monde.

use dofus_damage::{DamageProfile, Element, ElementStats, Resistance};
use dofus_engine::*;
use dofus_ruleset::{snapshot::Snapshot, Ruleset};

fn classe(nom: &str, breed: u32) -> Ruleset {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let mut rs = Ruleset::load(format!("{root}/data/rulesets/{nom}.yaml"))
        .unwrap_or_else(|e| panic!("{e}"));
    rs.merge_snapshot(
        &Snapshot::load(format!("{root}/data/snapshots/breed-{breed}.json")).unwrap(),
    );
    rs
}

fn moteur(rs: &Ruleset, deck: &[&str], cibles: u8) -> Engine {
    moteur_profil(
        rs,
        deck,
        cibles,
        [ElementStats {
            characteristic: 700,
            flat_damage: 90,
        }; 5],
        12,
    )
}

/// Un build dont un seul element porte, les autres restant au minimum.
fn moteur_mono(rs: &Ruleset, deck: &[&str], cibles: u8, dominant: Element, pa: u8) -> Engine {
    let mut elements = [ElementStats {
        characteristic: 50,
        flat_damage: 0,
    }; 5];
    elements[dominant.index()] = ElementStats {
        characteristic: 900,
        flat_damage: 90,
    };
    moteur_profil(rs, deck, cibles, elements, pa)
}

fn moteur_profil(
    rs: &Ruleset,
    deck: &[&str],
    cibles: u8,
    elements: [ElementStats; 5],
    pa: u8,
) -> Engine {
    let build = Build {
        name: "banc".into(),
        profile: DamageProfile {
            power: 200,
            flat_crit_damage: 120,
            elements,
            ..Default::default()
        },
        base_ap: pa,
        base_mp: 3,
        crit_bonus_percent: 30,
        modifiers: vec![],
        deck: deck.iter().map(|s| s.to_string()).collect(),
    };
    let sc = Scenario {
        poussees_bloquees: false,
        horizon: 3,
        pm_depenses: 0,
        etalement: 0,
        placement: None,
        etats_declares: vec![],
        targets: cibles,
        starting_turn_is_odd: true,
        resistance: Resistance::NONE,
        budgets: vec![],
        dominance: true,
        prune_spells: false,
        mode: Mode::Expected,
        reach: Reach::Ranged,
    };
    Engine::new(rs, build, sc).unwrap_or_else(|e| panic!("{e}"))
}

/// Un sort mono-cible gagne un lancer, puis n'avance plus. L'Intimidation part
/// trois fois par tour et deux fois sur la même cible : en duel c'est la limite
/// par cible qui mord, dès qu'un second ennemi est là c'est la limite par tour.
/// Six ennemis ne valent pas plus que deux.
#[test]
fn un_sort_mono_cible_gagne_un_lancer_puis_plafonne() {
    let rs = classe("iop", 8);
    let une = moteur(&rs, &["intimidation"], 1).solve().total;
    let deux = moteur(&rs, &["intimidation"], 2).solve().total;
    assert!(
        deux > une,
        "le troisieme lancer devient jouable a deux ennemis : {une} contre {deux}"
    );
    // Deux lancers contre trois : la moitie en plus, pas davantage.
    let rapport = deux.as_f64() / une.as_f64();
    assert!(
        (1.4..1.6).contains(&rapport),
        "deux lancers passent a trois, soit un rapport de 1,5 : mesure {rapport:.3}"
    );
    for cibles in [3u8, 6] {
        assert_eq!(
            moteur(&rs, &["intimidation"], cibles).solve().total,
            deux,
            "la limite par tour est atteinte des deux ennemis : {cibles} n'ajoute rien"
        );
    }
}

/// Un sort de zone monte, et s'arrete a la capacite de sa zone.
#[test]
fn un_sort_de_zone_monte_puis_plafonne() {
    // La Cadence du Roublard est une croix de rayon 1, soit cinq cases
    // (`shape: X, param1: 1`). Pas un sort du Iop : ils sont tous mono-cible côté
    // zone, l'Épée du Jugement comprise, dont le masque « Tous sauf lanceur »
    // désigne qui peut être touché, pas combien de cases sont couvertes.
    let rs = classe("roublard", 13);
    let mut totaux = Vec::new();
    for cibles in [1u8, 2, 3, 12] {
        totaux.push(moteur(&rs, &["cadence"], cibles).solve().total);
    }
    assert!(
        totaux[1] > totaux[0],
        "un sort de zone doit monter du premier au second ennemi : {totaux:?}"
    );
    assert!(
        totaux[2] > totaux[1],
        "et du second au troisieme : {totaux:?}"
    );
    assert!(
        totaux[3] >= totaux[2],
        "le plafond de zone borne, il ne fait pas redescendre : {totaux:?}"
    );
    // Et le plafond BORNE vraiment : une croix couvre cinq cases, douze
    // ennemis annonces ne peuvent pas valoir plus que cinq.
    let cinq = moteur(&rs, &["cadence"], 5).solve().total;
    assert_eq!(
        totaux[3], cinq,
        "au-dela de cinq cases la Cadence ne monte plus : {totaux:?} contre {cinq}"
    );
}

/// Le coeur du sujet : la rotation CHANGE quand les ennemis se multiplient.
///
/// Un deck ou un mono-cible bat un sort de zone a une cible doit basculer en
/// faveur de la zone quand ils sont plusieurs. Verifie sur les lancers eux-
/// memes et pas sur le total, un total plus gros pouvant venir du seul facteur
/// d'echelle sans qu'aucune decision ait bouge.
#[test]
fn la_rotation_bascule_vers_la_zone() {
    let rs = classe("roublard", 13);
    let deck = &["mousquet", "cadence", "arquebuse"][..];
    let compter = |cibles: u8, sort: &str| -> usize {
        moteur(&rs, deck, cibles)
            .solve()
            .turns
            .iter()
            .flat_map(|t| t.casts.iter())
            .filter(|c| c.id == sort)
            .count()
    };
    let zone_a_une = compter(1, "cadence");
    let zone_a_six = compter(6, "cadence");
    assert!(
        zone_a_six >= zone_a_une,
        "le sort de zone ne doit pas reculer quand les ennemis se multiplient : \
         {zone_a_une} lancer(s) sur une cible, {zone_a_six} sur six"
    );
    let total_une = moteur(&rs, deck, 1).solve().total;
    let total_six = moteur(&rs, deck, 6).solve().total;
    assert!(
        total_six > total_une,
        "six ennemis doivent valoir plus qu'un : {total_une} contre {total_six}"
    );
}

/// Un deck entièrement mono-cible plafonne vite : les sorts du Iop ne gagnent
/// que les lancers que leur limite par tour autorise, et la courbe est plate dès
/// le deuxième ennemi. C'est ce qui prouve que le plafond de zone est lu, et pas
/// seulement le nombre demandé : sinon huit ennemis vaudraient huit fois un.
#[test]
fn un_deck_mono_cible_plafonne_des_deux_ennemis() {
    let rs = classe("iop", 8);
    let deck = &["intimidation", "epee_du_jugement", "pression"][..];
    let une = moteur(&rs, deck, 1).solve().total;
    let deux = moteur(&rs, deck, 2).solve().total;
    assert!(
        deux > une,
        "un lancer de plus devient jouable : {une} / {deux}"
    );
    // Loin des huit fois qu'un deck de zone rendrait : la limite par tour tient.
    assert!(
        deux.as_f64() < une.as_f64() * 1.3,
        "un deck mono-cible ne suit pas le nombre d'ennemis : {une} contre {deux}"
    );
    for cibles in [3u8, 4, 8] {
        assert_eq!(
            moteur(&rs, deck, cibles).solve().total,
            deux,
            "aucun sort du Iop ne couvre plus d'une case : {cibles} ennemis ne \
             valent pas plus que deux"
        );
    }
}

/// Et le solveur échange un sort mono-cible contre un sort de zone.
///
/// L'échange ne se produit que si le sort de zone est individuellement plus
/// faible : le build est mono-élément Air, ce qui laisse le Pulsar, Feu, loin
/// derrière l'Arquebuse sur une cible. Il faut regarder la boucle, pas les tours
/// de l'horizon, où retarder un sort ne coûte rien. Et les PA doivent manquer :
/// à douze PA le Roublard lance tout ce que ses plafonds autorisent et il n'y a
/// rien à arbitrer ; sept PA rendent les deux sorts concurrents.
#[test]
fn le_solveur_echange_le_mono_cible_contre_la_zone() {
    let rs = classe("roublard", 13);
    let deck = &["arquebuse", "pulsar"][..];
    let dans_la_boucle = |cibles: u8, sort: &str| {
        moteur_mono(&rs, deck, cibles, Element::Air, 7)
            .steady_state()
            .cycle
            .iter()
            .flat_map(|t| t.casts.iter())
            .filter(|c| c.id == sort)
            .count()
    };
    let arq_une = dans_la_boucle(1, "arquebuse");
    let arq_huit = dans_la_boucle(8, "arquebuse");
    let zone_huit = dans_la_boucle(8, "pulsar");
    assert!(
        arq_une > 0,
        "l'Arquebuse doit gagner sur une cible, sinon le test ne montre rien"
    );
    assert!(
        arq_huit < arq_une,
        "sur huit ennemis le Pulsar doit prendre sa place dans la boucle : {arq_une} \
         Arquebuse(s) sur une cible, {arq_huit} sur huit, dont {zone_huit} Pulsar"
    );
}

/// Un deck MIXTE monte moins vite que la proportionnalite.
///
/// C'est la lecture que la page propose au joueur : l'ecart entre sa courbe et
/// la droite pointillee dit quelle part de ses degats ne profite pas du nombre.
/// Si un deck mixte montait proportionnellement, cet ecart serait toujours nul
/// et le graphique ne dirait rien.
#[test]
fn un_deck_mixte_monte_moins_vite_que_la_proportionnalite() {
    let rs = classe("roublard", 13);
    let deck = &["arquebuse", "pulsar"][..];
    let une = moteur_mono(&rs, deck, 1, Element::Air, 7).solve().total;
    let quatre = moteur_mono(&rs, deck, 4, Element::Air, 7).solve().total;
    assert!(quatre > une, "quatre ennemis doivent valoir plus qu'un");
    assert!(
        quatre.0 < une.0 * 4,
        "une part des degats vient de sorts mono-cible : quatre ennemis ne peuvent pas \
         valoir quatre fois un ({une:?} contre {quatre:?})"
    );
}
