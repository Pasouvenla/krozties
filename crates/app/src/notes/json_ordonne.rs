//! Un JSON qui garde l'ordre de ses clés, et s'écrit comme
//! `json.dumps(indent=2, ensure_ascii=False)` de Python, dont sortent les
//! relevés de `data/snapshots`. serde_json range les clés par ordre
//! alphabétique : un relevé relu puis réécrit changerait sur des milliers de
//! lignes. Ici il reste identique à l'octet près (un test le vérifie sur les
//! dix-neuf), et un changement de note ne touche que sa ligne.

use serde::de::{Deserialize, Deserializer, MapAccess, SeqAccess, Visitor};

#[derive(Debug, Clone, PartialEq)]
pub enum Json {
    Nul,
    Bool(bool),
    Nombre(serde_json::Number),
    Texte(String),
    Liste(Vec<Json>),
    Objet(Vec<(String, Json)>),
}

impl Json {
    pub fn lire(texte: &str) -> Result<Json, String> {
        serde_json::from_str(texte).map_err(|e| e.to_string())
    }

    /// Le texte tel que Python l'écrit, retour à la ligne final compris.
    pub fn ecrire(&self) -> String {
        let mut s = String::new();
        self.ecrire_dans(&mut s, 0);
        s.push('\n');
        s
    }

    fn ecrire_dans(&self, s: &mut String, niveau: usize) {
        let retrait = |s: &mut String, n: usize| s.push_str(&"  ".repeat(n));
        match self {
            Json::Nul => s.push_str("null"),
            Json::Bool(b) => s.push_str(if *b { "true" } else { "false" }),
            Json::Nombre(n) => s.push_str(&n.to_string()),
            Json::Texte(t) => s.push_str(&serde_json::to_string(t).unwrap_or_default()),
            Json::Liste(v) if v.is_empty() => s.push_str("[]"),
            Json::Liste(v) => {
                s.push_str("[\n");
                for (i, e) in v.iter().enumerate() {
                    retrait(s, niveau + 1);
                    e.ecrire_dans(s, niveau + 1);
                    s.push_str(if i + 1 < v.len() { ",\n" } else { "\n" });
                }
                retrait(s, niveau);
                s.push(']');
            }
            Json::Objet(m) if m.is_empty() => s.push_str("{}"),
            Json::Objet(m) => {
                s.push_str("{\n");
                for (i, (k, v)) in m.iter().enumerate() {
                    retrait(s, niveau + 1);
                    s.push_str(&serde_json::to_string(k).unwrap_or_default());
                    s.push_str(": ");
                    v.ecrire_dans(s, niveau + 1);
                    s.push_str(if i + 1 < m.len() { ",\n" } else { "\n" });
                }
                retrait(s, niveau);
                s.push('}');
            }
        }
    }

    pub fn champ(&self, cle: &str) -> Option<&Json> {
        match self {
            Json::Objet(m) => m.iter().find(|(k, _)| k == cle).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn champ_mut(&mut self, cle: &str) -> Option<&mut Json> {
        match self {
            Json::Objet(m) => m.iter_mut().find(|(k, _)| k == cle).map(|(_, v)| v),
            _ => None,
        }
    }

    pub fn liste_mut(&mut self) -> Option<&mut Vec<Json>> {
        match self {
            Json::Liste(v) => Some(v),
            _ => None,
        }
    }

    pub fn entier(&self) -> Option<i64> {
        match self {
            Json::Nombre(n) => n.as_i64(),
            _ => None,
        }
    }

    pub fn booleen(&self) -> Option<bool> {
        match self {
            Json::Bool(b) => Some(*b),
            _ => None,
        }
    }

    pub fn texte(&self) -> Option<&str> {
        match self {
            Json::Texte(t) => Some(t),
            _ => None,
        }
    }
}

impl<'de> Deserialize<'de> for Json {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Json, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = Json;

            fn expecting(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
                f.write_str("une valeur JSON")
            }
            fn visit_unit<E>(self) -> Result<Json, E> {
                Ok(Json::Nul)
            }
            fn visit_none<E>(self) -> Result<Json, E> {
                Ok(Json::Nul)
            }
            fn visit_bool<E>(self, b: bool) -> Result<Json, E> {
                Ok(Json::Bool(b))
            }
            fn visit_i64<E>(self, n: i64) -> Result<Json, E> {
                Ok(Json::Nombre(n.into()))
            }
            fn visit_u64<E>(self, n: u64) -> Result<Json, E> {
                Ok(Json::Nombre(n.into()))
            }
            fn visit_f64<E: serde::de::Error>(self, n: f64) -> Result<Json, E> {
                serde_json::Number::from_f64(n).map(Json::Nombre).ok_or_else(|| E::custom("nombre non fini"))
            }
            fn visit_str<E>(self, t: &str) -> Result<Json, E> {
                Ok(Json::Texte(t.to_string()))
            }
            fn visit_string<E>(self, t: String) -> Result<Json, E> {
                Ok(Json::Texte(t))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Json, A::Error> {
                let mut v = Vec::new();
                while let Some(e) = a.next_element()? {
                    v.push(e);
                }
                Ok(Json::Liste(v))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Json, A::Error> {
                let mut m = Vec::new();
                while let Some((k, v)) = a.next_entry::<String, Json>()? {
                    m.push((k, v));
                }
                Ok(Json::Objet(m))
            }
        }
        d.deserialize_any(V)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// ⚠️ UN RELEVÉ RELU PUIS RÉÉCRIT EST IDENTIQUE À L'OCTET PRÈS, pour les
    /// dix-neuf classes : sans quoi une note changerait des milliers de lignes
    /// pour une valeur, et la relecture d'une PR deviendrait impossible.
    #[test]
    fn les_releves_font_l_aller_retour_a_l_identique() {
        let racine = concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/snapshots");
        let mut vus = 0;
        for (classe, _, _) in crate::solve::CLASSES {
            let chemin = format!("{racine}/breed-{classe}.json");
            let texte = std::fs::read_to_string(&chemin).unwrap();
            let json = Json::lire(&texte).unwrap();
            assert!(json.ecrire() == texte, "{chemin} change à la réécriture");
            vus += 1;
        }
        assert_eq!(vus, 19);
    }

    #[test]
    fn l_ordre_des_cles_et_les_echappements_tiennent() {
        let texte = "{\n  \"z\": 1,\n  \"a\": \"é \\\" \\n\",\n  \"m\": [],\n  \"o\": {}\n}\n";
        assert_eq!(Json::lire(texte).unwrap().ecrire(), texte);
    }
}
