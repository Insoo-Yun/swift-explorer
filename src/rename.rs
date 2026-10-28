//! Toplu yeniden adlandırma: kurala göre yeni adlar, çakışma denetimi ve uygulama.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Clone, Copy, PartialEq, Eq, Default)]
pub enum Case {
    #[default]
    Keep,
    Lower,
    Upper,
    Title,
}

#[derive(Clone, Default)]
pub struct Rule {
    pub find: String,
    pub replace: String,
    /// Boşsa ad olduğu gibi kalır. `{name}` eski ad, `{n}` sıra numarası, `{date}` değiştirme tarihi.
    pub template: String,
    pub start: u32,
    pub digits: u32,
    pub case: Case,
}

pub struct Item {
    pub path: PathBuf,
    pub is_dir: bool,
    pub modified: Option<SystemTime>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Problem {
    Invalid,
    Duplicate,
    Exists,
}

fn split_ext(name: &str, is_dir: bool) -> (&str, &str) {
    match name.rfind('.') {
        Some(i) if !is_dir && i > 0 => (&name[..i], &name[i..]),
        _ => (name, ""),
    }
}

fn title_case(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut start = true;
    for c in s.chars() {
        if start {
            out.extend(c.to_uppercase());
        } else {
            out.extend(c.to_lowercase());
        }
        start = !c.is_alphanumeric();
    }
    out
}

/// Uzantı korunur; kural yalnız adın uzantıdan önceki kısmına uygulanır
pub fn new_name(rule: &Rule, item: &Item, index: usize) -> String {
    let name = item.path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let (stem, ext) = split_ext(&name, item.is_dir);
    let mut stem = if rule.find.is_empty() { stem.to_string() } else { stem.replace(&rule.find, &rule.replace) };
    if !rule.template.is_empty() {
        let n = format!("{:0width$}", rule.start as usize + index, width = rule.digits as usize);
        let date = item.modified.map(|t| chrono::DateTime::<chrono::Local>::from(t).format("%Y-%m-%d").to_string()).unwrap_or_default();
        stem = rule.template.replace("{name}", &stem).replace("{n}", &n).replace("{date}", &date);
    }
    let stem = match rule.case {
        Case::Keep => stem,
        Case::Lower => stem.to_lowercase(),
        Case::Upper => stem.to_uppercase(),
        Case::Title => title_case(&stem),
    };
    format!("{}{ext}", stem.trim())
}

/// Her öğenin yeni adı ve varsa sorunu
pub fn plan(rule: &Rule, items: &[Item]) -> Vec<(String, Option<Problem>)> {
    let names: Vec<String> = items.iter().enumerate().map(|(i, it)| new_name(rule, it, i)).collect();
    // Windows'ta adlar büyük/küçük harfe duyarsız
    let key = |dir: &Path, n: &str| (dir.to_path_buf(), if cfg!(windows) { n.to_lowercase() } else { n.to_string() });
    let mut seen: HashMap<(PathBuf, String), usize> = HashMap::new();
    for (it, n) in items.iter().zip(&names) {
        *seen.entry(key(it.path.parent().unwrap_or(Path::new("")), n)).or_default() += 1;
    }
    let sources: HashSet<(PathBuf, String)> = items
        .iter()
        .map(|it| key(it.path.parent().unwrap_or(Path::new("")), &it.path.file_name().unwrap_or_default().to_string_lossy()))
        .collect();
    items
        .iter()
        .zip(names)
        .map(|(it, n)| {
            let dir = it.path.parent().unwrap_or(Path::new(""));
            let k = key(dir, &n);
            let problem = if !crate::app::valid_name(&n) {
                Some(Problem::Invalid)
            } else if seen[&k] > 1 {
                Some(Problem::Duplicate)
            } else if !sources.contains(&k) && dir.join(&n).exists() {
                Some(Problem::Exists)
            } else {
                None
            };
            (n, problem)
        })
        .collect()
}

/// Önce geçici adlara, sonra yeni adlara taşır ki "a→b, b→a" gibi takaslar da çalışsın.
/// Bir adım başarısız olursa yapılanlar geri alınır.
pub fn apply(pairs: &[(PathBuf, PathBuf)]) -> Result<(), String> {
    let pairs: Vec<(&PathBuf, &PathBuf, PathBuf)> = pairs
        .iter()
        .filter(|(a, b)| a != b)
        .enumerate()
        .map(|(i, (a, b))| (a, b, a.with_file_name(format!(".swift-rename-{}-{i}", std::process::id()))))
        .collect();
    let mut parked = 0;
    for (a, _, t) in &pairs {
        if let Err(e) = std::fs::rename(a, t) {
            for (a, _, t) in &pairs[..parked] {
                let _ = std::fs::rename(t, a);
            }
            return Err(e.to_string());
        }
        parked += 1;
    }
    for (i, (_, b, t)) in pairs.iter().enumerate() {
        if let Err(e) = std::fs::rename(t, b) {
            for (_, b, t) in &pairs[..i] {
                let _ = std::fs::rename(b, t);
            }
            for (a, _, t) in &pairs {
                let _ = std::fs::rename(t, a);
            }
            return Err(e.to_string());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(p: &str) -> Item {
        Item { path: PathBuf::from(p), is_dir: false, modified: None }
    }

    #[test]
    fn find_replace_keeps_extension() {
        let r = Rule { find: "IMG".into(), replace: "Tatil".into(), ..Rule::default() };
        assert_eq!(new_name(&r, &item("d/IMG_01.IMG.jpg"), 0), "Tatil_01.Tatil.jpg");
    }

    #[test]
    fn template_numbers_and_case() {
        let r = Rule { template: "{name} {n}".into(), start: 1, digits: 3, case: Case::Title, ..Rule::default() };
        assert_eq!(new_name(&r, &item("d/ankara gezisi.png"), 0), "Ankara Gezisi 001.png");
        assert_eq!(new_name(&r, &item("d/x.png"), 9), "X 010.png");
        let up = Rule { case: Case::Upper, ..Rule::default() };
        assert_eq!(new_name(&up, &item("d/abc.txt"), 0), "ABC.txt");
        let dir = Item { path: PathBuf::from("d/a.b"), is_dir: true, modified: None };
        assert_eq!(new_name(&up, &dir, 0), "A.B");
    }

    #[test]
    fn plan_finds_duplicates_and_invalid_names() {
        let r = Rule { template: "same".into(), ..Rule::default() };
        let p = plan(&r, &[item("d/a.txt"), item("d/b.txt")]);
        assert!(p.iter().all(|(_, pr)| *pr == Some(Problem::Duplicate)));
        let bad = Rule { template: "a:b".into(), ..Rule::default() };
        assert_eq!(plan(&bad, &[item("d/a.txt")])[0].1, Some(Problem::Invalid));
    }

    #[test]
    fn apply_swaps_names_and_plan_allows_it() {
        let root = std::env::temp_dir().join(format!("swift-explorer-rename-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.txt"), "A").unwrap();
        std::fs::write(root.join("b.txt"), "B").unwrap();
        std::fs::write(root.join("c.txt"), "C").unwrap();
        // b → c çakışır, c listede değil
        let r = Rule { find: "b".into(), replace: "c".into(), ..Rule::default() };
        assert_eq!(plan(&r, &[item(root.join("b.txt").to_str().unwrap())])[0].1, Some(Problem::Exists));
        let pairs = [(root.join("a.txt"), root.join("b.txt")), (root.join("b.txt"), root.join("a.txt"))];
        apply(&pairs).unwrap();
        assert_eq!(std::fs::read_to_string(root.join("a.txt")).unwrap(), "B");
        assert_eq!(std::fs::read_to_string(root.join("b.txt")).unwrap(), "A");
        std::fs::remove_dir_all(&root).unwrap();
    }
}
