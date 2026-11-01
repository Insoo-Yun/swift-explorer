//! Komut paletinde yazılan metni komut adlarıyla eşleştirme.

/// Küçük harfe çevirir; Türkçe harfleri ve İ/ı farkını yok sayar ki
/// "klasor" da "Klasör" ile eşleşsin.
fn fold(s: &str) -> Vec<char> {
    s.chars()
        .flat_map(char::to_lowercase)
        .filter(|&c| c != '\u{307}')
        .map(|c| match c {
            'ı' => 'i',
            'ş' => 's',
            'ğ' => 'g',
            'ç' => 'c',
            'ö' => 'o',
            'ü' => 'u',
            _ => c,
        })
        .collect()
}

fn word_start(s: &[char], i: usize) -> bool {
    i == 0 || !s[i - 1].is_alphanumeric()
}

fn term_score(t: &[char], s: &[char]) -> Option<i32> {
    // Bitişik geçiş en iyisi; kelime başındaysa daha da iyi
    let best = (0..s.len().saturating_sub(t.len()) + 1)
        .filter(|&i| s[i..].starts_with(t))
        .map(|i| 1000 + if word_start(s, i) { 500 } else { 0 } - i as i32)
        .max();
    if best.is_some() {
        return best;
    }
    // Yoksa harfler sırayla geçmeli: "ykl" → "Yeni klasör"
    let mut score = 0;
    let mut j = 0;
    let mut prev: Option<usize> = None;
    for &c in t {
        let i = (j..s.len()).find(|&i| s[i] == c)?;
        score += 10;
        if word_start(s, i) {
            score += 30;
        }
        if prev == Some(i.wrapping_sub(1)) {
            score += 20;
        }
        prev = Some(i);
        j = i + 1;
    }
    Some(score)
}

/// Eşleşmezse None. Boşlukla ayrılan her parça adın içinde geçmeli.
pub fn score(query: &str, label: &str) -> Option<i32> {
    let s = fold(label);
    query.split_whitespace().try_fold(0, |acc, term| Some(acc + term_score(&fold(term), &s)?))
}

/// "Kes (Ctrl+X)" ya da "Geri al      Ctrl+Z" biçimindeki menü metnini ad ve kısayol olarak ayırır
pub fn split_shortcut(s: &str) -> (&str, &str) {
    if let Some(i) = s.strip_suffix(')').and_then(|r| r.rfind(" (")) {
        return (&s[..i], &s[i + 2..s.len() - 1]);
    }
    match s.find("  ") {
        Some(i) => (&s[..i], s[i..].trim()),
        None => (s, ""),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_menu_labels() {
        assert_eq!(split_shortcut("Kes (Ctrl+X)"), ("Kes", "Ctrl+X"));
        assert_eq!(split_shortcut("Back (Alt+Left Arrow)"), ("Back", "Alt+Left Arrow"));
        assert_eq!(split_shortcut("Geri al                   Ctrl+Z"), ("Geri al", "Ctrl+Z"));
        assert_eq!(split_shortcut("元に戻す                   Ctrl+Z"), ("元に戻す", "Ctrl+Z"));
        assert_eq!(split_shortcut("Terminalde aç"), ("Terminalde aç", ""));
    }

    #[test]
    fn empty_query_matches_everything() {
        assert_eq!(score("", "Yenile"), Some(0));
        assert_eq!(score("   ", "Yenile"), Some(0));
    }

    #[test]
    fn substring_and_subsequence() {
        assert!(score("klas", "Yeni klasör").is_some());
        assert!(score("ykl", "Yeni klasör").is_some());
        assert!(score("yeni kl", "Yeni klasör").is_some());
        assert_eq!(score("xyz", "Yeni klasör"), None);
        assert_eq!(score("yeni zz", "Yeni klasör"), None);
    }

    #[test]
    fn turkish_letters_fold() {
        assert!(score("klasor", "Klasör boyutlarını hesapla").is_some());
        assert!(score("ileri", "İleri").is_some());
        assert!(score("IŞIK", "ışık").is_some());
    }

    #[test]
    fn word_start_ranks_higher() {
        let start = score("sil", "Sil").unwrap();
        let inside = score("sil", "Kalıcı silme").unwrap();
        let middle = score("sil", "Fosil").unwrap();
        assert!(start > middle);
        assert!(inside > middle);
        assert!(score("gizli", "Gizli öğeler").unwrap() > score("gzl", "Gizli öğeler").unwrap());
    }
}
