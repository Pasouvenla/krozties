//! Calibrated Dofus damage pipeline, applied per damage line, never per cast:
//!
//! ```text
//!   scaled = floor(base * (100 + characteristic + power) / 100)
//!   inner  = scaled + flat_elemental [+ flat_critical, on a critical hit]
//!   final  = floor(inner * product(multiplicative final-damage modifiers))
//!   after  = max(0, floor(final * (100 - res_percent) / 100) - res_flat)
//! ```
//!
//! * `flat_elemental` already includes all-element flat damage: adding it again
//!   overstates a four-element spell by about 6%.
//! * `flat_critical` applies once per line: a four-line spell adds it four times.
//! * The final-damage percentages add up into one factor (see [`FinalMultiplier`]).
//!
//! Every stage is integer arithmetic, so a floor never lands on the wrong side
//! of an integer. Not verified in game: the order of the percentage and flat
//! resistance components, and the clamp.

#![forbid(unsafe_code)]

use std::fmt;
use std::ops::{Add, AddAssign};

/// Fixed-point scale for expected values: an `i64` scaled by 10^4 compares
/// exactly, which Pareto dominance needs and floating point cannot promise.
pub const SCALE: i64 = 10_000;

// ---------------------------------------------------------------------------
// Elements
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "lowercase"))]
pub enum Element {
    Fire,
    Earth,
    Air,
    Water,
    /// Neutral scales off Strength like Earth, but has its own flat damage and its
    /// own resistance.
    Neutral,
}

impl Element {
    pub const ALL: [Element; 5] = [
        Element::Fire,
        Element::Earth,
        Element::Air,
        Element::Water,
        Element::Neutral,
    ];

    #[inline]
    pub const fn index(self) -> usize {
        self as usize
    }
}

// ---------------------------------------------------------------------------
// Build-side damage profile
// ---------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct ElementStats {
    /// Intelligence, Strength, Agility or Chance, whichever drives this element.
    pub characteristic: i32,
    /// Per-element flat damage, all-element damage included.
    pub flat_damage: i32,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct DamageProfile {
    pub power: i32,
    /// Flat critical damage, applied once per damage line on a critical hit.
    pub flat_crit_damage: i32,
    pub elements: [ElementStats; 5],
    /// `% Dommages aux sorts`, in whole points: 6 means +6%. Effect 2812 (2813
    /// negative).
    pub percent_spell: i32,
    /// `% Dommages d'armes`. Effect 2808 (2809 negative).
    pub percent_weapon: i32,
    /// `% Dommages mêlée`. Effect 2800 (2801 negative).
    pub percent_melee: i32,
    /// `% Dommages distance`. Effect 2804 (2805 negative).
    pub percent_ranged: i32,
    /// Points de vie maximum du lanceur : 50, plus 5 par niveau, plus la Vitalité.
    /// Seuls les dégâts en pourcentage de vie la lisent.
    pub life: i32,
    /// `Dommages Poussée`, en points : ne sert qu'aux dommages de poussée,
    /// voir [`degats_de_poussee`].
    pub push_damage: i32,
    /// Le niveau du lanceur, que lisent aussi les dommages de poussée.
    pub level: i32,
}

/// Les dommages qu'une entité prend quand sa poussée bute :
/// `(niveau / 2 + 32 + Dommages Poussée - Résistance Poussée) × cases / 4`, où
/// `cases` est ce que la poussée n'a pas pu faire ; une entité percutée prend la
/// même chose divisée par 8. Ni caractéristique, ni Puissance, ni pourcentage,
/// ni critique ; les divisions arrondissent vers le bas, et le résultat ne
/// descend pas sous zéro.
pub fn degats_de_poussee(
    niveau: i32,
    do_poussee: i32,
    re_poussee: i32,
    cases: i32,
    percutee: bool,
) -> i32 {
    if cases <= 0 {
        return 0;
    }
    let base = niveau / 2 + 32 + do_poussee - re_poussee;
    (base * cases / if percutee { 8 } else { 4 }).max(0)
}

/// Des dégâts bruts, ceux en pourcentage de vie du lanceur : ni caractéristique,
/// ni Puissance, ni dommages fixes, ni multiplicateur, ni critique. Seules les
/// résistances de l'élément les réduisent, en pourcentage puis en fixe.
pub fn raw_line(value: i32, element: Element, resistance: &Resistance) -> Damage {
    Damage(resistance.apply(i64::from(value), element) * SCALE)
}

/// What is being cast, which decides whether `% spell` or `% weapon` applies.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Delivery {
    Spell,
    Weapon,
}

/// How far the target is, which decides whether `% melee` or `% ranged`
/// applies: contact is melee, two cells or more is ranged.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(rename_all = "snake_case"))]
pub enum Reach {
    Melee,
    Ranged,
}

/// The two properties of a cast that select which percentage modifiers apply.
/// They belong to the cast, not the spell: the same spell cast at contact or from
/// afar takes different modifiers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct CastContext {
    pub delivery: Delivery,
    pub reach: Reach,
}

impl CastContext {
    pub const SPELL_MELEE: CastContext = CastContext {
        delivery: Delivery::Spell,
        reach: Reach::Melee,
    };
    pub const SPELL_RANGED: CastContext = CastContext {
        delivery: Delivery::Spell,
        reach: Reach::Ranged,
    };
    pub const WEAPON_MELEE: CastContext = CastContext {
        delivery: Delivery::Weapon,
        reach: Reach::Melee,
    };
    pub const WEAPON_RANGED: CastContext = CastContext {
        delivery: Delivery::Weapon,
        reach: Reach::Ranged,
    };
}

impl Default for CastContext {
    fn default() -> Self {
        CastContext::SPELL_RANGED
    }
}

impl DamageProfile {
    #[inline]
    pub fn stats(&self, element: Element) -> ElementStats {
        self.elements[element.index()]
    }
}

// ---------------------------------------------------------------------------
// Final-damage multiplier
// ---------------------------------------------------------------------------

/// The final-damage factor, kept as an exact rational so the floor that follows
/// never lands wrong.
///
/// ⚠️ Final-damage percentages ADD UP whatever their source (Dofus, equipment,
/// the caster's spells): they form a single factor. The other factors
/// (% spell or weapon, % melee or ranged, damage taken by the target) multiply.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct FinalMultiplier {
    num: i128,
    den: i128,
    /// La somme des % de dommages finaux : 30 pour +10 % et +20 %.
    finaux: i64,
}

impl FinalMultiplier {
    pub const NEUTRAL: FinalMultiplier = FinalMultiplier { num: 1, den: 1, finaux: 0 };

    /// Build from whole percentages, where 110 means +10%; they add up, so 110 and
    /// 120 make +30%. Pass only the modifiers whose condition holds this turn.
    pub fn from_percents(percents: &[u32]) -> FinalMultiplier {
        let finaux = percents.iter().map(|&p| i64::from(p) - 100).sum();
        FinalMultiplier { num: 1, den: 1, finaux }
    }

    /// Ajoute des % de dommages finaux, qui rejoignent la même somme : ceux
    /// d'un état du lanceur, comme les portails de l'Éliotrope.
    #[must_use]
    pub fn plus_finaux(self, bonus: i64) -> FinalMultiplier {
        FinalMultiplier { finaux: self.finaux + bonus, ..self }
    }

    /// Compose with one more multiplicative modifier, exactly.
    #[must_use]
    pub fn times(self, percent: u32) -> FinalMultiplier {
        FinalMultiplier {
            num: self.num * i128::from(percent),
            den: self.den * 100,
            finaux: self.finaux,
        }
    }

    /// Compose with a signed bonus: `6` means x1.06, `-15` means x0.85; zero leaves
    /// the multiplier untouched.
    #[must_use]
    pub fn times_bonus(self, bonus: i32) -> FinalMultiplier {
        if bonus == 0 {
            return self;
        }
        FinalMultiplier {
            num: self.num * i128::from((100 + bonus).max(0)),
            den: self.den * 100,
            finaux: self.finaux,
        }
    }

    /// Compose two multipliers.
    #[must_use]
    pub fn and(self, other: FinalMultiplier) -> FinalMultiplier {
        FinalMultiplier {
            num: self.num * other.num,
            den: self.den * other.den,
            finaux: self.finaux + other.finaux,
        }
    }

    /// The factor of the cast context: the attacker's `% spell` or `% weapon` and
    /// `% melee` or `% ranged`, and the target's matching resistances. They multiply
    /// with each other and with the final-damage factor, and the product is floored
    /// ONCE: with 5% spell, 2% ranged and 10% final damage on 360, flooring each step
    /// gives 423, flooring once gives 424, as the game does.
    ///
    /// Not verified in game: the stage of the target's domain resistances, modelled
    /// as the mirror of the attacker's.
    pub fn for_cast(
        profile: &DamageProfile,
        resistance: &Resistance,
        ctx: CastContext,
    ) -> FinalMultiplier {
        let (attack_delivery, resist_delivery) = match ctx.delivery {
            Delivery::Spell => (profile.percent_spell, resistance.percent_spell),
            Delivery::Weapon => (profile.percent_weapon, resistance.percent_weapon),
        };
        let (attack_reach, resist_reach) = match ctx.reach {
            Reach::Melee => (profile.percent_melee, resistance.percent_melee),
            Reach::Ranged => (profile.percent_ranged, resistance.percent_ranged),
        };
        FinalMultiplier::NEUTRAL
            .times_bonus(attack_delivery)
            .times_bonus(attack_reach)
            .times_bonus(-resist_delivery)
            .times_bonus(-resist_reach)
    }

    #[inline]
    pub fn apply(self, value: i64) -> i64 {
        let finaux = i128::from((100 + self.finaux).max(0));
        floor_div(i128::from(value) * self.num * finaux, self.den * 100)
    }
}

impl Default for FinalMultiplier {
    fn default() -> Self {
        FinalMultiplier::NEUTRAL
    }
}

// ---------------------------------------------------------------------------
// Target resistance
// ---------------------------------------------------------------------------

/// Per-element target resistance. The flat part is subtracted per damage line,
/// so a four-element spell loses it four times.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[cfg_attr(feature = "serde", serde(default))]
pub struct Resistance {
    pub percent: [i32; 5],
    pub flat: [i32; 5],
    /// `% Résistance aux sorts`, in whole points: 6 multiplies incoming spell damage
    /// by 0.94. Effect 2815 (2814 negative).
    pub percent_spell: i32,
    /// `% Résistance aux armes`. Effect 2811 (2810 negative).
    pub percent_weapon: i32,
    /// `% Résistance mêlée`. Effect 2803 (2802 negative).
    pub percent_melee: i32,
    /// `% Résistance distance`. Effect 2807 (2806 negative).
    pub percent_ranged: i32,
    /// Critical damage resistance, subtracted from the flat critical bonus of each
    /// line. Whether it applies per line or per cast is not verified in game;
    /// modelled per line, like the bonus.
    pub critical: i32,
}

impl Resistance {
    pub const NONE: Resistance = Resistance {
        percent: [0; 5],
        flat: [0; 5],
        critical: 0,
        percent_spell: 0,
        percent_weapon: 0,
        percent_melee: 0,
        percent_ranged: 0,
    };

    #[inline]
    fn apply(&self, value: i64, element: Element) -> i64 {
        let i = element.index();
        if self.percent[i] == 0 && self.flat[i] == 0 {
            return value;
        }
        let reduced = floor_div(i128::from(value) * i128::from(100 - self.percent[i]), 100);
        (reduced - i64::from(self.flat[i])).max(0)
    }
}

// ---------------------------------------------------------------------------
// Damage values
// ---------------------------------------------------------------------------

/// An expected damage value, scaled by [`SCALE`].
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default, Debug, Hash)]
pub struct Damage(pub i64);

impl Damage {
    pub const ZERO: Damage = Damage(0);

    #[inline]
    pub const fn from_int(v: i64) -> Damage {
        Damage(v * SCALE)
    }

    #[inline]
    pub fn as_f64(self) -> f64 {
        self.0 as f64 / SCALE as f64
    }

    /// Rounded to the nearest whole point of damage, for display.
    #[inline]
    pub fn round(self) -> i64 {
        floor_div(
            i128::from(self.0) + i128::from(SCALE / 2),
            i128::from(SCALE),
        )
    }
}

impl Add for Damage {
    type Output = Damage;
    #[inline]
    fn add(self, rhs: Damage) -> Damage {
        Damage(self.0 + rhs.0)
    }
}

impl AddAssign for Damage {
    #[inline]
    fn add_assign(&mut self, rhs: Damage) {
        self.0 += rhs.0;
    }
}

impl std::iter::Sum for Damage {
    fn sum<I: Iterator<Item = Damage>>(iter: I) -> Damage {
        Damage(iter.map(|d| d.0).sum())
    }
}

impl fmt::Display for Damage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1}", self.as_f64())
    }
}

// ---------------------------------------------------------------------------
// Spell lines
// ---------------------------------------------------------------------------

/// One damage line of a spell or weapon. Normal and critical hits have separate
/// base ranges, on top of the flat critical damage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct SpellLine {
    pub element: Element,
    /// Inclusive base damage range on a normal hit.
    pub normal: (i32, i32),
    /// Inclusive base damage range on a critical hit.
    pub critical: (i32, i32),
}

/// Probability that a cast lands a critical hit, in permille. Per spell (base
/// rates from 0 to 30, plus the character's bonus); a spell that cannot crit is
/// [`CritRate::NEVER`], not a rate of zero.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct CritRate(i32);

impl CritRate {
    pub const NEVER: CritRate = CritRate(0);
    pub const ALWAYS: CritRate = CritRate(1000);

    /// From whole percentages, clamped to [0, 100].
    pub fn from_percent(p: i32) -> CritRate {
        CritRate(p.clamp(0, 100) * 10)
    }

    #[inline]
    pub const fn permille(self) -> i32 {
        self.0
    }
}

// ---------------------------------------------------------------------------
// The pipeline
// ---------------------------------------------------------------------------

/// Resolve one damage line for one specific base roll and one specific outcome.
///
/// This is the only place the formula lives.
pub fn resolve(
    base: i32,
    element: Element,
    profile: &DamageProfile,
    multiplier: FinalMultiplier,
    critical: bool,
    resistance: &Resistance,
) -> i64 {
    let stats = profile.stats(element);
    let scale = 100 + stats.characteristic + profile.power;

    let scaled = floor_div(i128::from(base) * i128::from(scale), 100);
    let mut inner = scaled + i64::from(stats.flat_damage);
    if critical {
        inner += i64::from((profile.flat_crit_damage - resistance.critical).max(0));
    }

    let after_multiplier = multiplier.apply(inner);
    resistance.apply(after_multiplier, element)
}

/// Expected damage for one line, over the base roll and the crit outcome. Every
/// roll goes through the pipeline separately: `E[floor(x)]` is not `floor(E[x])`,
/// and the gap matters on small lines once flat resistance clamps at zero.
pub fn expected_line(
    line: &SpellLine,
    profile: &DamageProfile,
    multiplier: FinalMultiplier,
    crit_rate: CritRate,
    resistance: &Resistance,
) -> Damage {
    let p = i128::from(crit_rate.permille());

    let (crit_sum, crit_n) = roll_sum(
        line.critical,
        line.element,
        profile,
        multiplier,
        true,
        resistance,
    );
    let (norm_sum, norm_n) = roll_sum(
        line.normal,
        line.element,
        profile,
        multiplier,
        false,
        resistance,
    );

    // (p * crit_mean + (1000 - p) * normal_mean) * SCALE / 1000, exactly.
    let num = i128::from(SCALE) * (p * crit_sum * norm_n + (1000 - p) * norm_sum * crit_n);
    let den = 1000 * crit_n * norm_n;
    Damage(round_div(num, den))
}

/// Prototype-parity value: midpoint of the critical range, always critical, no
/// resistance. Only for checking the engine against the Python prototypes; it
/// overstates every spell and is not an objective.
pub fn compat_midpoint_crit(
    line: &SpellLine,
    profile: &DamageProfile,
    multiplier: FinalMultiplier,
) -> Damage {
    let lo = resolve(
        line.critical.0,
        line.element,
        profile,
        multiplier,
        true,
        &Resistance::NONE,
    );
    let hi = resolve(
        line.critical.1,
        line.element,
        profile,
        multiplier,
        true,
        &Resistance::NONE,
    );
    Damage((lo + hi) * SCALE / 2)
}

/// Min and max whole-point damage for a line, for display.
pub fn range(
    line: &SpellLine,
    profile: &DamageProfile,
    multiplier: FinalMultiplier,
    critical: bool,
    resistance: &Resistance,
) -> (i64, i64) {
    let (lo, hi) = if critical { line.critical } else { line.normal };
    (
        resolve(lo, line.element, profile, multiplier, critical, resistance),
        resolve(hi, line.element, profile, multiplier, critical, resistance),
    )
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn roll_sum(
    range: (i32, i32),
    element: Element,
    profile: &DamageProfile,
    multiplier: FinalMultiplier,
    critical: bool,
    resistance: &Resistance,
) -> (i128, i128) {
    let (lo, hi) = range;
    debug_assert!(lo <= hi, "damage range is inverted: {lo}..{hi}");
    let mut sum: i128 = 0;
    for base in lo..=hi {
        sum += i128::from(resolve(
            base, element, profile, multiplier, critical, resistance,
        ));
    }
    (sum, i128::from(hi - lo + 1))
}

#[inline]
fn floor_div(num: i128, den: i128) -> i64 {
    debug_assert!(den > 0);
    let q = num.div_euclid(den);
    i64::try_from(q).expect("damage value overflowed i64")
}

#[inline]
fn round_div(num: i128, den: i128) -> i64 {
    debug_assert!(den > 0);
    floor_div(num + den / 2, den)
}
