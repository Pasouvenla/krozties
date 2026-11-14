//! Where a fight takes place, read from the text the game writes on screen:
//! the map's coordinates, `x,y` with either sign, and its sub-area.
//!
//! The text itself comes from the system's text recognition, on the machine
//! ([`crate::text`]). Where the game writes it on the window is not known
//! yet: the reading takes whatever lines it is given, finds the coordinates
//! among them, and takes for the sub-area the line of letters just above
//! them, or the words before them on their own line.

/// A line of text read on a frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub text: String,
    /// How sure the recognition was, from 0 to 1.
    pub confidence: f32,
    /// Left, top, width and height, in frame pixels.
    pub bounds: (f32, f32, f32, f32),
}

/// Where a fight takes place.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Place {
    pub coords: (i32, i32),
    /// The sub-area, when a line of letters goes with the coordinates.
    pub subarea: Option<String>,
}

/// Map coordinates stay within this, either way.
const FARTHEST: i32 = 200;

/// The text with the minus signs the game or the recognition may write
/// turned into hyphens.
fn plain(text: &str) -> String {
    text.chars().map(|c| if matches!(c, '\u{2212}' | '\u{2013}' | '\u{2014}' | '\u{2010}') { '-' } else { c }).collect()
}

/// A signed number ending at byte `end` of `text`, read backwards, and where
/// it starts.
fn number_before(text: &str, end: usize) -> Option<(i32, usize)> {
    let head = text[..end].trim_end();
    let digits = head.len() - head.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let mut start = head.len() - digits;
    let mut value: i32 = head[start..].parse().ok()?;
    if head[..start].ends_with('-') {
        start -= 1;
        value = -value;
    }
    Some((value, start))
}

/// A signed number starting at byte `start` of `text`, and where it ends.
fn number_after(text: &str, start: usize) -> Option<(i32, usize)> {
    let tail = &text[start..];
    let trimmed = tail.trim_start();
    let mut at = start + (tail.len() - trimmed.len());
    let negative = trimmed.starts_with('-');
    if negative {
        at += 1;
    }
    let digits = text[at..].len() - text[at..].trim_start_matches(|c: char| c.is_ascii_digit()).len();
    if digits == 0 {
        return None;
    }
    let value: i32 = text[at..at + digits].parse().ok()?;
    Some((if negative { -value } else { value }, at + digits))
}

/// The coordinates in a line, and the text before them.
fn coords_in(text: &str) -> Option<((i32, i32), String)> {
    let text = plain(text);
    for (comma, _) in text.match_indices(',') {
        let (Some((x, start)), Some((y, end))) = (number_before(&text, comma), number_after(&text, comma + 1)) else {
            continue;
        };
        // A number glued to letters or other digits is part of something else.
        let glued = |c: Option<char>| c.is_some_and(|c| c.is_alphanumeric());
        if glued(text[..start].chars().next_back()) || glued(text[end..].chars().next()) {
            continue;
        }
        if x.abs() <= FARTHEST && y.abs() <= FARTHEST {
            return Some(((x, y), text[..start].to_string()));
        }
    }
    None
}

/// A sub-area name from the words of a line: what is left once brackets,
/// separators and numbers are set aside, if letters are left.
fn name_in(text: &str) -> Option<String> {
    let kept: String = text.chars().map(|c| if c.is_alphabetic() || c == '\'' || c == '-' { c } else { ' ' }).collect();
    let name = kept.split_whitespace().filter(|w| w.chars().any(char::is_alphabetic)).collect::<Vec<_>>().join(" ");
    (name.chars().filter(|c| c.is_alphabetic()).count() >= 3).then_some(name)
}

/// The place written in `lines`, if their text holds map coordinates.
pub fn place(lines: &[Line]) -> Option<Place> {
    let (at, coords, before) = lines.iter().enumerate().find_map(|(k, l)| coords_in(&l.text).map(|(c, b)| (k, c, b)))?;
    let line = &lines[at];
    let (left, top, width, _) = line.bounds;
    let height = line.bounds.3.max(1.0);
    // Above the coordinates, in the same column, within two lines' height.
    let above = lines
        .iter()
        .enumerate()
        .filter(|(k, l)| {
            let (x, y, w, h) = l.bounds;
            *k != at && y + h <= top + height * 0.25 && top - (y + h) <= 2.0 * height && x < left + width && left < x + w
        })
        .max_by(|a, b| (a.1.bounds.1 + a.1.bounds.3).total_cmp(&(b.1.bounds.1 + b.1.bounds.3)))
        .and_then(|(_, l)| name_in(&l.text));
    Some(Place { coords, subarea: name_in(&before).or(above) })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn line(text: &str, top: f32) -> Line {
        Line { text: text.into(), confidence: 1.0, bounds: (20.0, top, 240.0, 24.0) }
    }

    #[test]
    fn the_coordinates_under_the_sub_area() {
        let lines = [line("Plaine des Scarafeuilles", 10.0), line("4,-12", 38.0)];
        assert_eq!(place(&lines), Some(Place { coords: (4, -12), subarea: Some("Plaine des Scarafeuilles".into()) }));
    }

    #[test]
    fn the_coordinates_after_the_sub_area_on_one_line() {
        let lines = [line("Astrub (5, -18)", 10.0)];
        assert_eq!(place(&lines), Some(Place { coords: (5, -18), subarea: Some("Astrub".into()) }));
        let lines = [line("Île de Moon - [35 ,12]", 10.0)];
        assert_eq!(place(&lines), Some(Place { coords: (35, 12), subarea: Some("Île de Moon".into()) }));
    }

    /// The words on the line of the coordinates name the sub-area better
    /// than the line above, which may name the area around it.
    #[test]
    fn the_words_before_the_coordinates_come_first() {
        let lines = [line("Amakna", 10.0), line("Plaine des Scarafeuilles 4,-12", 38.0)];
        assert_eq!(place(&lines).and_then(|p| p.subarea), Some("Plaine des Scarafeuilles".into()));
    }

    #[test]
    fn any_minus_sign_is_a_minus() {
        assert_eq!(place(&[line("−3,−7", 0.0)]).map(|p| p.coords), Some((-3, -7)));
        assert_eq!(place(&[line("–20 , 4", 0.0)]).map(|p| p.coords), Some((-20, 4)));
    }

    /// Numbers that are not coordinates: a time, a level, a long number, a
    /// decimal; nothing above the coordinates in their column.
    #[test]
    fn other_numbers_are_not_coordinates() {
        for text in ["12:34", "Niveau 200", "1,2345", "x1,2", "3,5kg", "1234,5", "500,3", "99999999999,1"] {
            assert_eq!(place(&[line(text, 0.0)]), None, "{text}");
        }
        let far = Line { text: "Bonta".into(), confidence: 1.0, bounds: (600.0, 0.0, 100.0, 24.0) };
        assert_eq!(place(&[far, line("7,3", 30.0)]), Some(Place { coords: (7, 3), subarea: None }));
        let high = line("Brâkmar", 0.0);
        assert_eq!(place(&[high, line("7,3", 120.0)]).and_then(|p| p.subarea), None);
    }
}
