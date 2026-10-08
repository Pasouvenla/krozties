//! KrozBoom : les bombes du Roublard, leurs murs et leurs explosions.
//!
//! * trois bombes au plus, comme en jeu ;
//! * deux bombes du même élément sur une même ligne, à sept cases au plus l'une
//!   de l'autre, tendent un mur sur les cases qui les séparent. Les bombes
//!   liées de proche en proche forment un réseau, et une bombe d'un autre
//!   élément entre les deux ne coupe pas le mur ;
//! * le mur frappe de sa base, pleine au tour du Roublard et réduite hors de
//!   son tour, montée par la caractéristique, la Puissance et les dommages de
//!   l'élément, multipliée par la MOITIÉ des combos des bombes du réseau, puis
//!   par les % de dommages aux sorts, finaux, et de mêlée ou de distance ;
//! * chaque bombe explose en cercle de deux cases, en perdant 10 % par case
//!   d'éloignement. Son combo compte aussi celui des autres bombes à deux cases
//!   ou moins, ou de son réseau. L'explosion ne prend AUCUN % de dommages aux
//!   sorts, à distance ou finaux. Les explosions qui couvrent une même case
//!   s'additionnent : c'est le coup de toutes les bombes posées.
//!
//! Les bases sont celles de la donnée du jeu : les quatre murs (sorts 13458,
//! 13461, 13465, 13501) et l'explosion Feu (sort 13455). Les explosions Air, Eau
//! et Terre, que la donnée range dans des états, sont celles du devblog 2.61.
//!
//! Arithmétique entière, comme le reste du moteur : 20 × 1,15 fait 23, là où un
//! calcul en flottants tomberait à 22,999…

use crate::cartes::Plateau;
use dofus_damage::{Element, FinalMultiplier};
use dofus_engine::Case;

/// Trois bombes au plus sur le terrain.
pub const BOMBES_MAX: usize = 3;

/// L'identifiant de classe du Roublard.
pub(crate) const ROUBLARD: u32 = 13;

/// Le rayon de l'explosion, en cases.
const RAYON_EXPLOSION: i16 = 2;

/// Un mur se tend jusqu'à la septième case : six cases entre les bombes.
const ECART_MUR_MAX: i16 = 7;

/// Ce que chaque rang de combo ajoute aux dommages, en pour cent : du combo I,
/// qui n'ajoute rien, au combo XV. La table du sort « Combo » de la donnée
/// (24306).
pub const COMBOS: [i64; 15] = [0, 20, 40, 60, 80, 100, 120, 140, 160, 190, 220, 250, 280, 320, 360];

/// L'élément d'une bombe.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Deserialize, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ElementBombe {
    Feu,
    Air,
    Eau,
    Terre,
}

impl ElementBombe {
    /// Le nom de ses bombes, au pluriel : « Explobombes ».
    const fn nom_pluriel(self) -> &'static str {
        match self {
            Self::Feu => "Explobombes",
            Self::Air => "Tornabombes",
            Self::Eau => "Bombes à Eau",
            Self::Terre => "Sismobombes",
        }
    }

    /// Le sort qui la pose, par son identifiant dans la donnée : son icône, et
    /// les niveaux de ses rangs.
    pub const fn sort(self) -> u32 {
        match self {
            Self::Feu => 13444,
            Self::Air => 13435,
            Self::Eau => 13436,
            Self::Terre => 13491,
        }
    }

    /// Les niveaux où le sort passe aux rangs 2 et 3 (`minPlayerLevel` de la
    /// donnée).
    const fn paliers(self) -> [u32; 2] {
        match self {
            Self::Feu => [66, 132],
            Self::Air => [67, 133],
            Self::Eau => [68, 134],
            Self::Terre => [69, 136],
        }
    }

    /// L'élément de ses dommages, pour lire la caractéristique et les
    /// dommages du build.
    const fn element(self) -> Element {
        match self {
            Self::Feu => Element::Fire,
            Self::Air => Element::Air,
            Self::Eau => Element::Water,
            Self::Terre => Element::Earth,
        }
    }

    /// Le rang du sort à ce niveau, de 1 à 3.
    fn rang(self, niveau: u32) -> usize {
        1 + self.paliers().iter().filter(|p| niveau >= **p).count()
    }

    /// La base de l'explosion à ce rang.
    const fn explosion(self, rang: usize) -> (i64, i64) {
        match (self, rang) {
            (Self::Terre, 1) => (12, 13),
            (Self::Terre, 2) => (16, 17),
            (Self::Terre, _) => (20, 22),
            (_, 1) => (9, 10),
            (_, 2) => (13, 14),
            (_, _) => (17, 19),
        }
    }

    /// La base du mur à ce rang : au tour du Roublard, puis hors de son tour.
    const fn mur(self, rang: usize) -> [(i64, i64); 2] {
        match (self, rang) {
            (Self::Feu, 1) => [(21, 24), (11, 12)],
            (Self::Feu, 2) => [(24, 27), (12, 14)],
            (Self::Feu, _) => [(30, 33), (15, 17)],
            (_, 1) => [(15, 18), (8, 9)],
            (_, 2) => [(18, 21), (9, 10)],
            (_, _) => [(24, 27), (12, 14)],
        }
    }
}

/// Le mur de bombes que Plombage redéclenche dans la Rotation : l'élément de
/// ses bombes et leurs combos, de I à XV. La Rotation ne pose pas de bombes
/// sur un plateau : le joueur dit lequel, et la page le reprend par défaut de
/// ce qui est posé dans KrozBoom.
#[derive(Clone, Debug, serde::Deserialize)]
pub struct MurDeBombes {
    pub element: ElementBombe,
    pub combos: Vec<u8>,
}

impl Default for MurDeBombes {
    /// Deux Explobombes tout juste posées.
    fn default() -> Self {
        MurDeBombes { element: ElementBombe::Feu, combos: vec![combo_de_depart(); 2] }
    }
}

/// Le combo d'un mur que Plombage redéclenche : la moitié des combos de ses
/// bombes, chacune montée d'un cran par Plombage avant qu'il ne déclenche le
/// mur, plafonné à XV.
fn combo_apres_plombage(niveaux: impl Iterator<Item = u8>) -> i64 {
    niveaux.map(|c| COMBOS[usize::from(c).min(COMBOS.len() - 1)]).sum::<i64>() / 2
}

impl MurDeBombes {
    /// La ligne que Plombage, lancé sur une bombe, frappe dans la Rotation : la
    /// base du mur à votre tour, au rang que le niveau donne, son combo en
    /// facteur, et jamais de critique : les coups critiques ne s'appliquent pas
    /// aux bombes. Le moteur l'arrondit comme KrozBoom, au point près.
    pub fn ligne_de_plombage(&self, niveau: u32) -> Result<serde_json::Value, String> {
        self.valider()?;
        let (lo, hi) = self.element.mur(self.element.rang(niveau))[0];
        Ok(serde_json::json!({
            "element": self.element.element(),
            "normal": [lo, hi],
            "critical": [lo, hi],
            "facteur": 100 + combo_apres_plombage(self.combos.iter().copied()),
            "sans_critique": true,
        }))
    }

    /// La ligne du mur qui frappe de lui-même la cible qui s'y tient : au début
    /// de chacun de ses tours, à la base hors de votre tour ; quand il se forme
    /// sur elle ou qu'un de vos sorts la déplace dans le mur, à la base de
    /// votre tour. Ses combos tels que posés, la moitié de leur somme comme pour
    /// tout mur, et jamais de critique.
    pub fn ligne_du_mur(&self, niveau: u32, a_votre_tour: bool) -> Result<serde_json::Value, String> {
        self.valider()?;
        let (lo, hi) = self.element.mur(self.element.rang(niveau))[usize::from(!a_votre_tour)];
        let combo = self.combos.iter().map(|c| COMBOS[usize::from(*c) - 1]).sum::<i64>() / 2;
        Ok(serde_json::json!({
            "element": self.element.element(),
            "normal": [lo, hi],
            "critical": [lo, hi],
            "facteur": 100 + combo,
            "sans_critique": true,
        }))
    }

    fn valider(&self) -> Result<(), String> {
        if !(2..=BOMBES_MAX).contains(&self.combos.len()) {
            return Err(format!("un mur tient entre deux et {BOMBES_MAX} bombes"));
        }
        if let Some(c) = self.combos.iter().find(|c| !(1..=15).contains(*c)) {
            return Err(format!("combo {c} : il va de I à XV"));
        }
        Ok(())
    }

    /// Ce que la fiche en dit : « Explobombes aux combos III et II ».
    pub fn en_clair(&self) -> String {
        let romains = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X", "XI", "XII", "XIII", "XIV", "XV"];
        let combos: Vec<&str> =
            self.combos.iter().map(|c| romains[usize::from((*c).clamp(1, 15)) - 1]).collect();
        let liste = match combos.split_last() {
            Some((dernier, debut)) if !debut.is_empty() => format!("{} et {dernier}", debut.join(", ")),
            _ => combos.join(""),
        };
        format!("{} aux combos {liste}", self.element.nom_pluriel())
    }
}

/// Une bombe posée.
#[derive(Clone, Copy, Debug, serde::Deserialize)]
pub struct BombePosee {
    pub case: (i16, i16),
    pub element: ElementBombe,
    /// Son combo, de 1 (I, celui d'une bombe tout juste posée) à 15 (XV).
    #[serde(default = "combo_de_depart")]
    pub combo: u8,
}

const fn combo_de_depart() -> u8 {
    1
}

#[derive(Clone, Debug, serde::Deserialize)]
pub struct RequeteBombes {
    /// Le build du Roublard : ses caractéristiques font les dégâts. Absent, un
    /// Roublard niveau 200 sans équipement.
    #[serde(default)]
    pub build: Option<dofus_build::BuildInput>,
    #[serde(default)]
    pub bombes: Vec<BombePosee>,
    #[serde(default)]
    pub ennemis: Vec<(i16, i16)>,
    /// Le Roublard frappe-t-il au contact : les murs prennent alors ses % de
    /// dommages de mêlée plutôt que de distance. Le joueur le dit, lui qui sait
    /// où il se tient.
    #[serde(default)]
    pub melee: bool,
    /// La carte de boss, par son identifiant dans le relevé ; absente, le damier.
    #[serde(default)]
    pub carte: Option<u16>,
    /// Le côté du damier vide, sans carte : quinze par défaut.
    #[serde(default)]
    pub damier: Option<u8>,
    /// Les bonus de Dofus que le joueur a écartés dans l'onglet Rotation.
    #[serde(default)]
    pub bonus_ecartes: Vec<String>,
    /// Combien de fois Plombage, lancé sur une bombe, redéclenche son mur ce
    /// tour : deux au plus, Plombage se lançant deux fois par tour.
    #[serde(default)]
    pub plombages: u8,
    /// La bombe que le joueur fait sauter, par sa place dans `bombes`. Absente,
    /// toutes les bombes posées partent ensemble.
    #[serde(default)]
    pub explose: Option<usize>,
}

/// Ce qu'une base devient avec la caractéristique, la Puissance et les
/// dommages de l'élément : la première étape des deux formules.
fn monte(base: i64, carac: i64, puissance: i64, dommages: i64) -> i64 {
    (base * (100 + carac + puissance)).div_euclid(100) + dommages
}

/// Une explosion : la base montée, multipliée par les combos cumulés, puis par
/// le taux de la case. Aucun % de dommages.
pub fn explosion(base: i64, carac: i64, puissance: i64, dommages: i64, combos: i64, taux: i64) -> i64 {
    (monte(base, carac, puissance, dommages) * (100 + combos) * taux).div_euclid(10_000)
}

/// Un mur : la base montée, multipliée par la moitié des combos du réseau et
/// par les % de dommages du lanceur, avec un seul arrondi, comme le moteur pour
/// tout sort. Le mur que la Rotation compte pour Plombage vaut ainsi celui-ci
/// au point près.
///
/// ⚠️ Trois facteurs, pas une somme : les % finaux font un facteur à eux seuls
/// (Vulbis et Nébuleux, +10 % et +20 %, donnent ×1,30), et les % aux sorts
/// multiplient par-dessus (le +7 % du Kaboom).
pub fn mur(base: i64, carac: i64, puissance: i64, dommages: i64, combos_du_mur: i64, mult: FinalMultiplier) -> i64 {
    mult.times(u32::try_from(100 + combos_du_mur).unwrap_or(0))
        .apply(monte(base, carac, puissance, dommages))
}

struct Bombe {
    case: Case,
    element: ElementBombe,
    /// Son combo, en pour cent de dommages.
    combo: i64,
    /// Son réseau de murs, par sa place dans la liste des réseaux.
    reseau: Option<usize>,
    /// La moitié des combos de son réseau, en pour cent.
    combo_mur: i64,
    /// Son combo et celui des bombes qui explosent avec elle.
    combo_explosion: i64,
}

/// Un réseau de murs d'un même élément : ses bombes, par ordre de pose, et les
/// cases de ses murs.
struct Mur {
    element: ElementBombe,
    bombes: Vec<usize>,
    cases: Vec<Case>,
}

/// Les cases du mur que tendent les bombes `a` et `b`, ou rien : il faut le
/// même élément, la même ligne ou la même colonne, sept cases d'écart au plus,
/// et aucune bombe du même élément entre elles. Une bombe d'un autre élément,
/// un obstacle ou un trou ne le coupent pas.
fn mur_entre(bombes: &[Bombe], a: usize, b: usize) -> Option<Vec<Case>> {
    let (de, vers, element) = (bombes[a].case, bombes[b].case, bombes[a].element);
    if a == b || bombes[b].element != element || (de.x != vers.x && de.y != vers.y) {
        return None;
    }
    let ecart = (vers.x - de.x).abs() + (vers.y - de.y).abs();
    if ecart > ECART_MUR_MAX {
        return None;
    }
    let (px, py) = ((vers.x - de.x).signum(), (vers.y - de.y).signum());
    let cases: Vec<Case> = (1..ecart).map(|k| Case::new(de.x + px * k, de.y + py * k)).collect();
    let coupe = bombes.iter().any(|o| o.element == element && cases.contains(&o.case));
    (!coupe).then_some(cases)
}

/// Les réseaux de murs : des bombes liées deux à deux par un mur, de proche en
/// proche. Une bombe sans mur n'en forme pas. Chaque bombe d'un réseau apprend
/// sa place et la moitié des combos qu'il réunit.
fn tendre_les_murs(bombes: &mut [Bombe]) -> Vec<Mur> {
    let mut murs: Vec<Mur> = Vec::new();
    for depart in 0..bombes.len() {
        if bombes[depart].reseau.is_some() {
            continue;
        }
        let mut membres = vec![depart];
        let mut cases = std::collections::BTreeSet::new();
        let mut suivant = 0;
        while let Some(&a) = membres.get(suivant) {
            for b in 0..bombes.len() {
                if let Some(entre) = mur_entre(bombes, a, b) {
                    cases.extend(entre);
                    if !membres.contains(&b) {
                        membres.push(b);
                    }
                }
            }
            suivant += 1;
        }
        if membres.len() < 2 {
            continue;
        }
        membres.sort_unstable();
        let moitie = membres.iter().map(|&b| bombes[b].combo).sum::<i64>() / 2;
        for &b in &membres {
            bombes[b].reseau = Some(murs.len());
            bombes[b].combo_mur = moitie;
        }
        murs.push(Mur { element: bombes[depart].element, bombes: membres, cases: cases.into_iter().collect() });
    }
    murs
}

/// Deux bombes sautent ensemble, et leurs explosions cumulent leurs combos,
/// quand elles sont à deux cases ou moins l'une de l'autre, ou du même réseau.
fn liees(a: &Bombe, b: &Bombe) -> bool {
    a.case.distance(b.case) <= 2 || (a.reseau.is_some() && a.reseau == b.reseau)
}

/// Le combo de chaque explosion : le sien, plus celui des bombes qui lui sont
/// liées.
fn cumuler_les_combos(bombes: &mut [Bombe]) {
    for i in 0..bombes.len() {
        let autres: i64 = (0..bombes.len())
            .filter(|&j| j != i && liees(&bombes[i], &bombes[j]))
            .map(|j| bombes[j].combo)
            .sum();
        bombes[i].combo_explosion = bombes[i].combo + autres;
    }
}

/// Les bombes qui sautent quand le joueur fait sauter `depart` : elle, puis de
/// proche en proche celles qui sont liées à une bombe qui saute.
///
/// ⚠️ Déduit des textes du jeu, qui parlent d'une bombe « déclenchée par un
/// effet d'explosion » (Poudre) et d'explosions qui déclenchent les bombes
/// « en zone » (Bombe Collante, Mégabombe) ; le souffle d'une bombe couvre deux
/// cases. À vérifier en jeu.
fn reaction_en_chaine(bombes: &[Bombe], depart: usize) -> Vec<bool> {
    let mut sautent = vec![false; bombes.len()];
    let mut file = vec![depart];
    sautent[depart] = true;
    while let Some(i) = file.pop() {
        for j in 0..bombes.len() {
            if !sautent[j] && liees(&bombes[i], &bombes[j]) {
                sautent[j] = true;
                file.push(j);
            }
        }
    }
    sautent
}

/// Les cases de l'explosion d'une bombe et leur taux, sur le sol du plateau.
fn souffle(centre: Case, plateau: &Plateau) -> Vec<(Case, i64)> {
    let mut cases = Vec::new();
    for dx in -RAYON_EXPLOSION..=RAYON_EXPLOSION {
        let reste = RAYON_EXPLOSION - dx.abs();
        for dy in -reste..=reste {
            let c = Case::new(centre.x + dx, centre.y + dy);
            if plateau.est_sol(c) {
                let d = i64::from(dx.abs() + dy.abs());
                cases.push((c, 100 - 10 * d));
            }
        }
    }
    cases
}

/// Ce que prend une case : l'explosion de toutes les bombes, le mur.
#[derive(Default)]
struct Coups {
    explosion: Option<(i64, i64)>,
    /// L'élément du mur, puis un déclenchement à votre tour, hors de votre
    /// tour, et par Plombage.
    mur: Option<(ElementBombe, (i64, i64), (i64, i64), (i64, i64))>,
}

pub fn bombes_json(requete: &RequeteBombes) -> Result<String, String> {
    let plateau = Plateau::de(requete.carte, requete.damier)?;
    if requete.bombes.len() > BOMBES_MAX {
        return Err(format!("{BOMBES_MAX} bombes au plus, comme en jeu"));
    }
    let case = |(x, y): (i16, i16)| Case::new(x, y);
    for (i, b) in requete.bombes.iter().enumerate() {
        if !plateau.est_sol(case(b.case)) {
            return Err(format!("la case ({}, {}) n'est pas du sol : une bombe ne s'y pose pas", b.case.0, b.case.1));
        }
        if !(1..=15).contains(&b.combo) {
            return Err(format!("combo {} : il va de I à XV", b.combo));
        }
        if requete.bombes[..i].iter().any(|a| a.case == b.case) {
            return Err(format!("deux bombes sur la case ({}, {})", b.case.0, b.case.1));
        }
        if requete.ennemis.contains(&b.case) {
            return Err(format!("une bombe et un ennemi sur la case ({}, {})", b.case.0, b.case.1));
        }
    }
    if requete.plombages > 2 {
        return Err("Plombage se lance deux fois par tour au plus : il ne redéclenche le mur que deux fois".into());
    }
    if requete.explose.is_some_and(|i| i >= requete.bombes.len()) {
        return Err("la bombe à faire sauter n'est pas posée".into());
    }

    // Le build du Roublard, ou un Roublard niveau 200 sans équipement.
    let importe = requete.build.is_some();
    let build = requete
        .build
        .clone()
        .unwrap_or_else(|| dofus_build::BuildInput { class: ROUBLARD, level: 200, ..Default::default() })
        .normalise();
    if build.class != ROUBLARD {
        return Err("KrozBoom calcule les bombes d'un Roublard : le build donné est d'une autre classe".into());
    }
    let resolu = crate::solve::resolve_build(&build)?;
    let profil = &resolu.profile;
    // ⚠️ Tous les % de dommages finaux montent le mur, ceux gagnés en combat
    // compris : ceux de l'équipement et ceux des Dofus du build, additionnés ;
    // au tour impair pour le Rêve Nébuleux, comme KrozZone ; sans les bonus que
    // le joueur a écartés dans l'onglet Rotation.
    let finaux: Vec<u32> = resolu
        .damage_multipliers
        .iter()
        .filter(|(n, _)| resolu.tours.get(n) != Some(&dofus_build::Tours::Pairs))
        .filter(|(n, _)| !requete.bonus_ecartes.contains(n))
        .map(|(_, p)| *p)
        .collect();
    let portee = if requete.melee { profil.percent_melee } else { profil.percent_ranged };
    let multiplicateur = FinalMultiplier::from_percents(&finaux)
        .and(FinalMultiplier::NEUTRAL.times_bonus(profil.percent_spell).times_bonus(portee));
    let stats = |e: ElementBombe| {
        let s = profil.elements[e.element().index()];
        (i64::from(s.characteristic), i64::from(profil.power), i64::from(s.flat_damage))
    };

    let mut bombes: Vec<Bombe> = requete
        .bombes
        .iter()
        .map(|b| Bombe {
            case: case(b.case),
            element: b.element,
            combo: COMBOS[usize::from(b.combo) - 1],
            reseau: None,
            combo_mur: 0,
            combo_explosion: 0,
        })
        .collect();
    let murs = tendre_les_murs(&mut bombes);
    cumuler_les_combos(&mut bombes);
    let sautent = match requete.explose {
        Some(i) => reaction_en_chaine(&bombes, i),
        None => vec![true; bombes.len()],
    };

    // Ce que prend chaque case, au sol du plateau.
    let mut coups: std::collections::BTreeMap<(i16, i16), Coups> = std::collections::BTreeMap::new();
    let mut souffles = Vec::new();
    for (b, &saute) in bombes.iter().zip(&sautent) {
        let rang = b.element.rang(build.level);
        let (lo, hi) = b.element.explosion(rang);
        let (carac, puissance, dommages) = stats(b.element);
        let cases = souffle(b.case, &plateau);
        // Une bombe qui ne saute pas garde son souffle à venir, sans rien
        // faire tomber.
        for &(c, taux) in cases.iter().filter(|_| saute) {
            let e = &mut coups.entry((c.x, c.y)).or_default().explosion;
            let (a, z) = e.unwrap_or((0, 0));
            *e = Some((
                a + explosion(lo, carac, puissance, dommages, b.combo_explosion, taux),
                z + explosion(hi, carac, puissance, dommages, b.combo_explosion, taux),
            ));
        }
        souffles.push(cases);
    }
    for m in &murs {
        let rang = m.element.rang(build.level);
        let [tour, hors] = m.element.mur(rang);
        let (carac, puissance, dommages) = stats(m.element);
        let combo = bombes[m.bombes[0]].combo_mur;
        // Plombage ajoute 1 Combo à chaque bombe du mur AVANT de le
        // redéclencher, une fois par bombe et par tour : ses deux
        // déclenchements partent à ce combo.
        let combo_plombage = combo_apres_plombage(m.bombes.iter().map(|&b| requete.bombes[b].combo));
        let frappe = |(lo, hi): (i64, i64), combo: i64| {
            (
                mur(lo, carac, puissance, dommages, combo, multiplicateur),
                mur(hi, carac, puissance, dommages, combo, multiplicateur),
            )
        };
        for c in m.cases.iter().filter(|c| plateau.est_sol(**c)) {
            coups.entry((c.x, c.y)).or_default().mur =
                Some((m.element, frappe(tour, combo), frappe(hors, combo), frappe(tour, combo_plombage)));
        }
    }

    let json_mur = |m: &Option<(ElementBombe, (i64, i64), (i64, i64), (i64, i64))>| {
        m.map(|(element, tour, hors, _)| {
            serde_json::json!({ "element": element, "tour": [tour.0, tour.1], "hors_tour": [hors.0, hors.1] })
        })
    };
    let json_explosion = |e: &Option<(i64, i64)>| e.map(|(a, z)| [a, z]);
    Ok(serde_json::json!({
        "build_importe": importe,
        "niveau": build.level,
        "melee": requete.melee,
        "plombages": requete.plombages,
        "explose": requete.explose,
        "pourcents_des_murs": {
            "sorts": profil.percent_spell,
            "portee": portee,
            "finaux": finaux.iter().map(|p| i64::from(*p) - 100).sum::<i64>(),
        },
        "bombes": bombes.iter().zip(&souffles).zip(&sautent).map(|((b, cases), saute)| serde_json::json!({
            "case": [b.case.x, b.case.y],
            "saute": saute,
            "element": b.element,
            "sort": b.element.sort(),
            "rang": b.element.rang(build.level),
            "combo": b.combo,
            "en_mur": b.reseau.is_some(),
            "combo_explosion": b.combo_explosion,
            "combo_mur": b.combo_mur,
            "explosion": cases.iter().map(|(c, t)| [i64::from(c.x), i64::from(c.y), *t]).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "murs": murs.iter().map(|m| serde_json::json!({
            "element": m.element,
            "bombes": m.bombes,
            "cases": m.cases.iter().filter(|c| plateau.est_sol(**c)).map(|c| [c.x, c.y]).collect::<Vec<_>>(),
            "combo": bombes[m.bombes[0]].combo_mur,
        })).collect::<Vec<_>>(),
        "cases": coups.iter().map(|((x, y), c)| serde_json::json!({
            "case": [x, y],
            "explosion": json_explosion(&c.explosion),
            "mur": json_mur(&c.mur),
        })).collect::<Vec<_>>(),
        "ennemis": requete.ennemis.iter().enumerate().map(|(i, &(x, y))| {
            let c = coups.get(&(x, y));
            serde_json::json!({
                "indice": i,
                "case": [x, y],
                "explosion": c.and_then(|c| json_explosion(&c.explosion)),
                "mur": c.and_then(|c| json_mur(&c.mur)),
                // Les déclenchements de Plombage du tour, additionnés.
                "plombage": c.and_then(|c| c.mur).filter(|_| requete.plombages > 0).map(|(_, _, _, (lo, hi))| {
                    let n = i64::from(requete.plombages);
                    [lo * n, hi * n]
                }),
            })
        }).collect::<Vec<_>>(),
    })
    .to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn calcule(requete: serde_json::Value) -> serde_json::Value {
        let r: RequeteBombes = serde_json::from_value(requete).expect("la requête doit se lire");
        serde_json::from_str(&bombes_json(&r).expect("le calcul doit aboutir")).unwrap()
    }

    fn bombe(x: i16, y: i16, element: &str, combo: u8) -> serde_json::Value {
        serde_json::json!({ "case": [x, y], "element": element, "combo": combo })
    }

    /// Les deux formules, sur des valeurs calculées à la main.
    /// Explosion : 17 monté de 800 d'Intelligence et 150 de Puissance fait
    /// 178, plus 60 de dommages, 238 ; à 60 % de combos et 90 % de taux,
    /// 238 × 1,6 × 0,9 = 342,72. Mur : 30 monté fait 315, plus 60, 375 ; à
    /// 30 % de combos et avec les % du lanceur, un seul arrondi.
    #[test]
    fn les_formules_de_l_explosion_et_du_mur() {
        assert_eq!(explosion(17, 800, 150, 60, 60, 90), 342);
        assert_eq!(explosion(19, 0, 0, 0, 0, 100), 19);
        assert_eq!(explosion(19, 0, 0, 0, 80, 80), 27);
        // 375 × 1,30 (combos) × 1,20 (sorts) × 1,15 (distance) × 1,10
        // (finaux) = 740,02, arrondi une seule fois.
        let mult = FinalMultiplier::from_percents(&[110])
            .and(FinalMultiplier::NEUTRAL.times_bonus(20).times_bonus(15));
        assert_eq!(mur(30, 800, 150, 60, 30, mult), 740);
        assert_eq!(mur(33, 0, 0, 0, 0, FinalMultiplier::NEUTRAL), 33);
        // Un produit exact entier, que des flottants manqueraient d'un point.
        assert_eq!(monte(20, 15, 0, 0), 23);
    }

    /// Deux bombes du même élément sur une ligne tendent un mur entre elles,
    /// jusqu'à sept cases d'écart ; à huit, ou d'éléments différents, rien.
    #[test]
    fn un_mur_se_tend_entre_deux_bombes_du_meme_element() {
        let vu = calcule(serde_json::json!({ "bombes": [bombe(-3, 0, "feu", 1), bombe(0, 0, "feu", 1)] }));
        assert_eq!(vu["murs"][0]["cases"], serde_json::json!([[-2, 0], [-1, 0]]), "{vu}");
        let vu = calcule(serde_json::json!({ "bombes": [bombe(-4, 0, "feu", 1), bombe(3, 0, "feu", 1)] }));
        assert_eq!(vu["murs"][0]["cases"].as_array().unwrap().len(), 6, "sept cases d'écart : {vu}");
        let vu = calcule(serde_json::json!({ "bombes": [bombe(-4, 0, "feu", 1), bombe(4, 0, "feu", 1)] }));
        assert_eq!(vu["murs"], serde_json::json!([]), "huit cases d'écart : {vu}");
        let vu = calcule(serde_json::json!({ "bombes": [bombe(-3, 0, "feu", 1), bombe(0, 0, "eau", 1)] }));
        assert_eq!(vu["murs"], serde_json::json!([]), "deux éléments : {vu}");
        // En diagonale, pas de mur : seules les lignes comptent.
        let vu = calcule(serde_json::json!({ "bombes": [bombe(0, 0, "air", 1), bombe(2, 2, "air", 1)] }));
        assert_eq!(vu["murs"], serde_json::json!([]), "diagonale : {vu}");
    }

    /// Trois bombes en équerre font un seul réseau, et chacune porte la
    /// moitié de la somme de leurs combos : (40 + 20 + 80) / 2 = 70 %.
    #[test]
    fn trois_bombes_font_un_reseau() {
        let vu = calcule(serde_json::json!({ "bombes": [
            bombe(0, 0, "terre", 3), bombe(3, 0, "terre", 2), bombe(3, 3, "terre", 5),
        ] }));
        let murs = vu["murs"].as_array().unwrap();
        assert_eq!(murs.len(), 1, "{vu}");
        assert_eq!(murs[0]["bombes"], serde_json::json!([0, 1, 2]), "{vu}");
        assert_eq!(murs[0]["cases"], serde_json::json!([[1, 0], [2, 0], [3, 1], [3, 2]]), "{vu}");
        assert_eq!(murs[0]["combo"], 70, "{vu}");
    }

    /// Le combo d'une explosion cumule celui des bombes à deux cases ou moins,
    /// et celui des bombes reliées par un mur, même lointaines.
    #[test]
    fn les_combos_se_cumulent() {
        // Feu (0,0) III et Air (2,0) II sont à deux cases ; Feu (5,0) V est
        // relié à la première par un mur, que l'Air entre elles ne coupe pas.
        let vu = calcule(serde_json::json!({ "bombes": [
            bombe(0, 0, "feu", 3), bombe(2, 0, "air", 2), bombe(5, 0, "feu", 5),
        ] }));
        let b = &vu["bombes"];
        assert_eq!(b[0]["combo_explosion"], 40 + 20 + 80, "{vu}");
        assert_eq!(b[1]["combo_explosion"], 20 + 40, "trois cases de la troisième, sans mur : {vu}");
        assert_eq!(b[2]["combo_explosion"], 80 + 40, "{vu}");
    }

    /// Un ennemi dans le mur et le souffle de deux bombes : le mur au rang 3,
    /// 30 à 33 à 30 % de combos, 39 à 42 ; hors du tour, 15 à 17, 19 à 22.
    /// Les deux explosions, à 60 % de combos : 24 à 27 à une case, 21 à 24 à
    /// deux cases, soit 45 à 51.
    #[test]
    fn un_ennemi_prend_le_mur_et_les_explosions() {
        let vu = calcule(serde_json::json!({
            "bombes": [bombe(0, 0, "feu", 3), bombe(3, 0, "feu", 2)],
            "ennemis": [[1, 0], [0, 3]],
        }));
        let e = &vu["ennemis"][0];
        assert_eq!(e["mur"]["tour"], serde_json::json!([39, 42]), "{vu}");
        assert_eq!(e["mur"]["hors_tour"], serde_json::json!([19, 22]), "{vu}");
        assert_eq!(e["explosion"], serde_json::json!([45, 51]), "{vu}");
        // Trois cases de la première bombe : hors de tout souffle, hors du mur.
        assert!(vu["ennemis"][1]["explosion"].is_null() && vu["ennemis"][1]["mur"].is_null(), "{vu}");
    }

    /// La bombe qu'on fait sauter emporte celles à deux cases ou moins, de
    /// proche en proche, et celles de son mur ; les autres restent. Sans
    /// choix, toutes partent ensemble.
    #[test]
    fn la_reaction_en_chaine() {
        // Feu (0,0) et Air (2,0) à deux cases ; Eau (6,0) à quatre de l'Air.
        let bombes = serde_json::json!([bombe(0, 0, "feu", 1), bombe(2, 0, "air", 1), bombe(6, 0, "eau", 1)]);
        let sautent = |explose: Option<usize>| -> Vec<bool> {
            let vu = calcule(serde_json::json!({ "bombes": bombes, "explose": explose, "ennemis": [[7, 0]] }));
            vu["bombes"].as_array().unwrap().iter().map(|b| b["saute"].as_bool().unwrap()).collect()
        };
        assert_eq!(sautent(None), [true, true, true]);
        assert_eq!(sautent(Some(0)), [true, true, false]);
        assert_eq!(sautent(Some(2)), [false, false, true]);
        // De proche en proche : (0,0), (2,0), (4,0), la première et la
        // troisième à quatre cases l'une de l'autre.
        let vu = calcule(serde_json::json!({
            "bombes": [bombe(0, 0, "feu", 1), bombe(2, 0, "air", 1), bombe(4, 0, "eau", 1)], "explose": 0,
        }));
        assert!(vu["bombes"].as_array().unwrap().iter().all(|b| b["saute"] == true), "{vu}");
        // Par le mur : deux Explobombes à six cases, liées.
        let vu = calcule(serde_json::json!({ "bombes": [bombe(0, 0, "feu", 1), bombe(6, 0, "feu", 1)], "explose": 1 }));
        assert!(vu["bombes"].as_array().unwrap().iter().all(|b| b["saute"] == true), "{vu}");
        // Un ennemi dans le seul souffle de la bombe qui reste ne prend rien.
        let vu = calcule(serde_json::json!({ "bombes": bombes, "explose": 0, "ennemis": [[7, 0], [1, 0]] }));
        assert!(vu["ennemis"][0]["explosion"].is_null(), "{vu}");
        assert!(!vu["ennemis"][1]["explosion"].is_null(), "{vu}");
        let toutes = calcule(serde_json::json!({ "bombes": bombes, "ennemis": [[7, 0]] }));
        assert!(!toutes["ennemis"][0]["explosion"].is_null(), "{toutes}");
        // Une bombe qui n'est pas posée ne saute pas.
        let r: RequeteBombes = serde_json::from_value(serde_json::json!({ "bombes": bombes, "explose": 3 })).unwrap();
        assert!(bombes_json(&r).unwrap_err().contains("pas posée"));
    }

    /// Le rang suit le niveau : au niveau 100, le Feu est au rang 2 (13 à 14),
    /// la Terre aussi (16 à 17) ; au niveau 60, tous au rang 1.
    #[test]
    fn le_rang_suit_le_niveau() {
        assert_eq!(ElementBombe::Feu.rang(100), 2);
        assert_eq!(ElementBombe::Feu.rang(132), 3);
        assert_eq!(ElementBombe::Terre.rang(135), 2);
        assert_eq!(ElementBombe::Air.rang(60), 1);
        let vu = calcule(serde_json::json!({
            "build": { "class": 13, "level": 100 },
            "bombes": [bombe(0, 0, "feu", 1)],
            "ennemis": [[0, 1]],
        }));
        // 13 à 14 à 90 %, sans combo : 11 à 12.
        assert_eq!(vu["ennemis"][0]["explosion"], serde_json::json!([11, 12]), "{vu}");
    }

    /// Ce qui ne se pose pas se refuse, en clair.
    #[test]
    fn ce_qui_ne_se_pose_pas_se_refuse() {
        let refus = |v: serde_json::Value| {
            bombes_json(&serde_json::from_value::<RequeteBombes>(v).unwrap()).unwrap_err()
        };
        let quatre = serde_json::json!({ "bombes": [
            bombe(0, 0, "feu", 1), bombe(2, 0, "feu", 1), bombe(4, 0, "feu", 1), bombe(6, 0, "feu", 1),
        ] });
        assert!(refus(quatre).contains("3 bombes au plus"));
        assert!(refus(serde_json::json!({ "bombes": [bombe(0, 0, "feu", 16)] })).contains("XV"));
        assert!(refus(serde_json::json!({ "bombes": [bombe(40, 0, "feu", 1)] })).contains("pas du sol"));
        assert!(refus(serde_json::json!({
            "build": { "class": 8, "level": 200 }, "bombes": [bombe(0, 0, "feu", 1)],
        }))
        .contains("Roublard"));
    }

    /// Les % de dommages finaux des Dofus montent le mur, et une case décochée
    /// de l'onglet Rotation les retire : un Roublard portant le Nébuleux et le
    /// Vulbis a +20 % au tour impair et +10 %, soit 30 % sur ses murs.
    #[test]
    fn les_dofus_montent_le_mur() {
        let mut objets = vec![0u32; 17];
        objets[15] = 6980;
        objets[16] = 8698;
        let build = serde_json::json!({ "class": 13, "level": 200, "items": objets });
        let vu = calcule(serde_json::json!({ "build": build, "bombes": [bombe(0, 0, "feu", 1)] }));
        assert_eq!(vu["pourcents_des_murs"]["finaux"], 30, "{vu}");
        let vu = calcule(serde_json::json!({
            "build": build, "bombes": [bombe(0, 0, "feu", 1)], "bonus_ecartes": ["Rouge Vermeil"],
        }));
        assert_eq!(vu["pourcents_des_murs"]["finaux"], 20, "{vu}");
    }

    /// Sur une carte, le souffle et le mur ne couvrent que le sol.
    #[test]
    fn sur_une_carte_seul_le_sol_prend() {
        let klime = crate::cartes::cartes().iter().find(|c| c.nom == "Klime").expect("la carte Klime");
        let plateau = Plateau::Carte(klime);
        let sol: Vec<Case> = plateau.sol().into_iter().collect();
        let centre = sol[sol.len() / 2];
        let vu = calcule(serde_json::json!({
            "carte": klime.id, "bombes": [bombe(centre.x, centre.y, "eau", 1)],
        }));
        for c in vu["bombes"][0]["explosion"].as_array().unwrap() {
            let (x, y) = (c[0].as_i64().unwrap() as i16, c[1].as_i64().unwrap() as i16);
            assert!(plateau.est_sol(Case::new(x, y)), "({x}, {y}) : {vu}");
        }
    }

    /// Un obstacle entre deux bombes ne coupe pas leur mur, vérifié en jeu :
    /// sur une vraie carte, deux bombes de part et d'autre d'un
    /// obstacle tendent leur mur, et l'ennemi au-delà de l'obstacle le prend.
    #[test]
    fn un_obstacle_ne_coupe_pas_le_mur() {
        let (carte, a, b, derriere) = crate::cartes::cartes()
            .iter()
            .find_map(|carte| {
                let plateau = Plateau::Carte(carte);
                let obstacles: std::collections::BTreeSet<Case> = plateau.murs().into_iter().collect();
                plateau.sol().into_iter().find_map(|a| {
                    [(0, -1), (0, 1), (-1, 0), (1, 0)].iter().find_map(|&(dx, dy)| {
                        (3..=ECART_MUR_MAX).find_map(|pas| {
                            let b = Case::new(a.x + dx * pas, a.y + dy * pas);
                            let entre: Vec<Case> = (1..pas).map(|k| Case::new(a.x + dx * k, a.y + dy * k)).collect();
                            let i = entre.iter().position(|c| obstacles.contains(c))?;
                            let derriere = *entre[i + 1..].iter().find(|c| plateau.est_sol(**c))?;
                            plateau.est_sol(b).then_some((carte, a, b, derriere))
                        })
                    })
                })
            })
            .expect("une carte avec un obstacle entre deux cases de sol alignées");
        let vu = calcule(serde_json::json!({
            "carte": carte.id,
            "bombes": [bombe(a.x, a.y, "feu", 1), bombe(b.x, b.y, "feu", 1)],
            "ennemis": [[derriere.x, derriere.y]],
        }));
        assert_eq!(vu["murs"].as_array().unwrap().len(), 1, "{} : {vu}", carte.nom);
        assert!(!vu["ennemis"][0]["mur"].is_null(), "l'ennemi derrière l'obstacle prend le mur : {vu}");
    }

    /// Plombage redéclenche le mur, deux fois par tour au plus, ses deux
    /// lancers, au combo qu'il ajoute d'abord à
    /// chaque bombe du mur : ses deux déclenchements valent le mur à votre
    /// tour, bombes montées d'un cran. Hors du mur, rien ; au-delà de deux,
    /// refusé.
    #[test]
    fn plombage_redeclenche_le_mur() {
        let pose = |c1: u8, c2: u8, plombages: u8| {
            calcule(serde_json::json!({
                "bombes": [bombe(-2, 0, "feu", c1), bombe(2, 0, "feu", c2)],
                "ennemis": [[0, 0], [0, 2]],
                "plombages": plombages,
            }))
        };
        let vu = pose(3, 2, 2);
        let monte = pose(4, 3, 0);
        let deux = |i: usize| monte["ennemis"][0]["mur"]["tour"][i].as_i64().unwrap() * 2;
        assert_eq!(vu["ennemis"][0]["plombage"], serde_json::json!([deux(0), deux(1)]), "{vu}");
        assert!(vu["ennemis"][1]["plombage"].is_null(), "hors du mur : {vu}");
        assert!(pose(3, 2, 0)["ennemis"][0]["plombage"].is_null());
        // Au combo XV, le cran de Plombage ne monte plus rien.
        let max = pose(15, 15, 1);
        assert_eq!(max["ennemis"][0]["plombage"], max["ennemis"][0]["mur"]["tour"], "{max}");
        let trop: RequeteBombes = serde_json::from_value(serde_json::json!({ "plombages": 3 })).unwrap();
        assert!(bombes_json(&trop).is_err());
    }
}
