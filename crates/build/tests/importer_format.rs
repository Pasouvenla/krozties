//! The importer's own payload has to resolve, not a hand-written approximation
//! of it: the bookmarklet sends what DofusBook sends (`carac`, `fm`).

use dofus_build::*;
use dofus_damage::Element;

const PAYLOAD: &str = r#"{
  "v": 1, "src": "dofusbook", "id": 1, "short": "XXXXX",
  "class": 5, "level": 200,
  "items": [34330,31761,32234,24035,31762,17575,34332,32236,34331,13673,0,8698,7043,6980,739,7754,7115],
  "carac": {"base_ag":0,"base_ch":398,"base_fo":0,"base_in":0,"base_sa":0,"base_vi":3,
            "scroll_ag":0,"scroll_ch":0,"scroll_fo":0,"scroll_in":0,"scroll_sa":0,"scroll_vi":0},
  "fm": {"a1":{"pa":1},"a2":{"cc":5},"am":{"cc":8},"ar":{"cc":6},"bo":{"cc":6},
         "br":{"dc":8},"ca":{"cc":7},"ce":{"cc":6},"ch":{"cc":6}},
  "fmGlobal": {"pa":1,"pm":1,"po":1},
  "boosts": [{"name":"Rêve Nébuleux","stat":"deg","percent":20,"class_id":null},
             {"name":"Bleu Turquoise","stat":"deg","percent":10,"class_id":null}]
}"#;

fn catalogue() -> Catalogue {
    Catalogue::load(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../data/snapshots/items.json"
    ))
    .unwrap()
}

#[test]
fn the_importers_own_payload_resolves() {
    let input: BuildInput =
        serde_json::from_str(PAYLOAD).expect("le format du favori doit être lu");
    let r = resolve(&input, &catalogue());
    // The same figures the hand-written input produced, which is the point:
    // translating field names must change nothing else.
    assert_eq!(
        r.profile.elements[Element::Water.index()].characteristic,
        1118
    );
    assert_eq!(r.profile.elements[Element::Water.index()].flat_damage, 131);
    assert_eq!(r.profile.flat_crit_damage, 167);
    assert_eq!(r.base_ap, 11);
    assert_eq!(r.crit_bonus_percent, 127);
}

/// Boosts come in three families that are not interchangeable. Reading Power as
/// a percentage would multiply damage by four and a half.
#[test]
fn boost_families_are_not_confused() {
    let mut input: BuildInput = serde_json::from_str(PAYLOAD).unwrap();
    input.boosts = vec![
        Boost {
            name: "Dofus".into(),
            stat: "deg".into(),
            percent: 20,
            class_id: None,
            ..Default::default()
        },
        Boost {
            name: "Harmonie".into(),
            stat: "dmg".into(),
            percent: 20,
            class_id: None,
            ..Default::default()
        },
        Boost {
            name: "Puissance".into(),
            stat: "pu".into(),
            percent: 350,
            class_id: None,
            ..Default::default()
        },
    ];
    let r = resolve(&input, &catalogue());

    // Le « Dofus » générique est un multiplicateur, ni l'Harmonie ni la Puissance
    // n'en sont ; on ne regarde que les Dofus que ce build porte (Nébuleux, Vulbis).
    assert!(r.damage_multipliers.contains(&("Dofus".to_string(), 120)), "{:?}", r.damage_multipliers);
    assert!(
        !r.damage_multipliers.iter().any(|(n, _)| n == "Harmonie" || n == "Puissance"),
        "{:?}",
        r.damage_multipliers
    );
    // Power is a characteristic: 30 from the set bonus, plus 350.
    assert_eq!(r.profile.power, 380);
    // Flat damage lands on every element, once each.
    assert_eq!(r.profile.elements[Element::Water.index()].flat_damage, 151);
    assert_eq!(r.profile.elements[Element::Fire.index()].flat_damage, 89);
}

/// A boost carrying a class id is one of that class's own spells. The solver
/// casts it and pays its AP, so counting it here as a permanent bonus would
/// count it twice and hide the cost.
#[test]
fn class_buffs_are_left_to_the_solver() {
    let mut input: BuildInput = serde_json::from_str(PAYLOAD).unwrap();
    input.boosts = vec![Boost {
        name: "Puissance".into(),
        stat: "pu".into(),
        percent: 350,
        class_id: Some(8),
        ..Default::default()
    }];
    let r = resolve(&input, &catalogue());
    assert_eq!(
        r.profile.power, 30,
        "le buff de classe ne doit pas être compté ici"
    );
    assert!(
        r.assumptions
            .iter()
            .any(|a| a.contains("Puissance") && a.contains("non compté ici")),
        "{:?}",
        r.assumptions
    );
}

/// The v2 payload, which the bookmarklet sends untranslated: its code is copied
/// into the bookmark at install time, so nothing is interpreted before it
/// arrives.
const RAW: &str = r#"{
  "v": 2, "src": "dofusbook",
  "stuff": {
    "id": 1, "name": "Test", "character_class": 5, "character_level": 200,
    "stuffItem": {"ch": 6988, "am": 6597, "a1": 6742, "a2": 5118, "ce": 6598, "bo": 3572,
                  "ca": 6990, "br": 6744, "ar": 6989, "fa": 2654, "mo": null,
                  "d1": 3330, "d2": 2829, "d3": 1040, "d4": 258, "d5": 1241, "d6": 3992},
    "stuffCarac": {"base_ch": 398}
  },
  "items_table": [{"id":6988,"official":34330},{"id":6597,"official":31761},{"id":6742,"official":32234},
            {"id":5118,"official":24035},{"id":6598,"official":31762},{"id":3572,"official":17575},
            {"id":6990,"official":34332},{"id":6744,"official":32236},{"id":6989,"official":34331},
            {"id":2654,"official":13673},{"id":3330,"official":8698},{"id":2829,"official":7043},
            {"id":1040,"official":6980},{"id":258,"official":739},{"id":1241,"official":7754},
            {"id":3992,"official":7115}],
  "fmItems": {"a1":{"pa":1},"a2":{"cc":5},"am":{"cc":8},"ar":{"cc":6},"bo":{"cc":6},
              "br":{"dc":8},"ca":{"cc":7},"ce":{"cc":6},"ch":{"cc":6}},
  "boosts": []
}"#;

#[test]
fn the_untranslated_payload_resolves_identically() {
    let input: BuildInput = serde_json::from_str(RAW).expect("le format v2 doit être lu");
    let r = resolve(&input, &catalogue());
    // Exactly what the translated payload produced, which is the whole point:
    // moving the translation server-side must change nothing else.
    assert_eq!(
        r.profile.elements[Element::Water.index()].characteristic,
        1118
    );
    assert_eq!(r.profile.elements[Element::Water.index()].flat_damage, 131);
    assert_eq!(r.profile.flat_crit_damage, 167);
    assert_eq!(r.base_ap, 11);
    assert_eq!(r.crit_bonus_percent, 127);
    assert!(
        !r.assumptions.iter().any(|a| a.contains("inconnu")),
        "aucun objet ne doit manquer : {:?}",
        r.assumptions
    );
}

/// Normalising twice must give the same thing as normalising once.
#[test]
fn normalising_is_idempotent() {
    let input: BuildInput = serde_json::from_str(RAW).unwrap();
    let une = resolve(&input, &catalogue());
    let deux = resolve(&input.normalise(), &catalogue());
    assert_eq!(
        une.profile.elements[Element::Water.index()].characteristic,
        1118
    );
    assert_eq!(
        une.profile.elements[Element::Water.index()].characteristic,
        deux.profile.elements[Element::Water.index()].characteristic
    );
    assert_eq!(une.base_ap, deux.base_ap);
    assert_eq!(une.profile.power, deux.profile.power);
}

/// Boosts arrive exactly as DofusBook writes them: `boostName`, `effectName`,
/// `effectValue` and `count`, the amount being the product of the last two.
#[test]
fn boosts_are_read_in_dofusbooks_own_shape() {
    let raw = r#"{
      "stuff": {"id": 1, "character_class": 5, "character_level": 200,
                "stuffItem": {"d3": 258}, "stuffCarac": {}},
      "items_table": [{"id": 258, "official": 739}],
      "boosts": [
        {"active": true, "boostName": "Bleu Turquoise", "effectName": "deg",
         "effectValue": 1, "count": 10, "classId": null},
        {"active": true, "boostName": "Puissance", "effectName": "pu",
         "effectValue": 350, "count": 1, "classId": 8},
        {"active": false, "boostName": "Éteint", "effectName": "deg",
         "effectValue": 50, "count": 1, "classId": null}
      ]
    }"#;
    let input: BuildInput = serde_json::from_str(raw).expect("le format brut doit être lu");
    let r = resolve(&input, &catalogue());

    // 1 x 10 = +10%, and only the active one: the Dofus Turquoise is worn, and a
    // Dofus bonus only counts with its Dofus.
    assert_eq!(
        r.damage_multipliers,
        vec![("Bleu Turquoise".to_string(), 110)]
    );
    // The class buff is left to the solver, which casts it and pays its AP.
    assert_eq!(r.profile.power, 0);
    assert!(r
        .assumptions
        .iter()
        .any(|a| a.contains("Puissance") && a.contains("non compté ici")));
}

/// DofusBook's `stuffItem` holds its own item ids, not Ankama's: the payload
/// carries `items_table` to translate them. Without it, a slot resolves to
/// nothing, or to an unrelated item sharing the number.
#[test]
fn untranslated_item_ids_are_reported_not_silently_resolved() {
    // The DofusBook-internal ids of a real build, passed off as Ankama ids.
    const INTERNES: &str = r#"{
      "v": 1, "src": "dofusbook", "class": 9, "level": 200,
      "items": [3570,2826,2819,2820,5776,2804,3855,2803,3571,258,236,2829,1241,1040,2619,2654,0],
      "carac": {"base_fo":200,"base_in":200,"base_vi":395,
                "scroll_fo":100,"scroll_in":100,"scroll_vi":100},
      "fm": {}, "fmGlobal": {}, "boosts": []
    }"#;
    let input: BuildInput = serde_json::from_str(INTERNES).unwrap();
    let r = resolve(&input, &catalogue());
    assert!(
        !r.items_missing.is_empty(),
        "des identifiants internes ne doivent pas passer pour des objets résolus"
    );

    // The same build with the table applied resolves whole.
    const TRADUIT: &str = r#"{
      "v": 2, "src": "dofusbook",
      "stuff": {"id": 19404444, "character_class": 9, "character_level": 200,
        "stuffItem": {"ch":3570,"am":2826,"ca":2819,"ce":2820,"ar":5776,"bo":2804,
                      "br":3855,"a1":2803,"a2":3571,"d1":258,"d2":236,"d3":2829,
                      "d4":1241,"d5":1040,"d6":2619,"fa":2654,"mo":null},
        "stuffCarac": {"base_fo":200,"base_in":200,"base_vi":395,
                       "scroll_fo":100,"scroll_in":100,"scroll_vi":100}},
      "items_table": [{"id":3570,"official":17573},{"id":2826,"official":14169},
                      {"id":2819,"official":14161},{"id":2820,"official":14162},
                      {"id":5776,"official":27644},{"id":2804,"official":14093},
                      {"id":3855,"official":18718},{"id":2803,"official":14092},
                      {"id":3571,"official":17574},{"id":258,"official":739},
                      {"id":236,"official":694},{"id":2829,"official":7043},
                      {"id":1241,"official":7754},{"id":1040,"official":6980},
                      {"id":2619,"official":13344},{"id":2654,"official":13673}],
      "fmItems": {}, "fmGlobal": {}, "boosts": []
    }"#;
    let traduit: BuildInput = serde_json::from_str(TRADUIT).unwrap();
    let t = resolve(&traduit, &catalogue());
    assert!(
        t.items_missing.is_empty(),
        "avec la table, tout doit se résoudre : {:?}",
        t.items_missing
    );
    assert!(
        t.crit_bonus_percent > r.crit_bonus_percent,
        "le build traduit doit porter plus de critique que le build fantôme \
         ({} contre {})",
        t.crit_bonus_percent,
        r.crit_bonus_percent
    );
}

/// La forgemagie élémentaire de l'arme (`fmWeapon`, « df-85 ») ne change que les
/// lignes Neutre : le Yaularc frappe Terre et Feu, et le reste.
#[test]
fn la_forgemagie_d_arme_se_lit_et_ne_touche_que_le_neutre() {
    let charge = r#"{
      "v": 1, "src": "dofusbook", "class": 9, "level": 200,
      "items": [0,0,0,0,0,0,0,0,27644,0,0,0,0,0,0,0,0],
      "fmWeapon": "df-85", "fmStealWeapon": null
    }"#;
    let build: BuildInput = serde_json::from_str(charge).unwrap();
    assert_eq!(build.fm_weapon.as_deref(), Some("df-85"));
    let arme = resolve(&build, &catalogue()).arme.expect("le Yaularc");
    assert_eq!(arme.nom, "Yaularc");
    let lignes: Vec<(&str, i32, i32)> = arme.arme.lines.iter().map(|l| (l.element.as_str(), l.min, l.max)).collect();
    assert_eq!(lignes, [("earth", 25, 30), ("fire", 25, 30)]);
    assert!(arme.forgemagie.is_empty(), "{:?}", arme.forgemagie);
}
