use std::collections::HashSet;
use std::path::Path;

pub struct Syntax {
    pub keywords: HashSet<String>,
    pub types: HashSet<String>,
    pub comment: String,
    pub string: char,
}

impl Syntax {
    pub fn load(file: &str) -> Option<Self> {
        let ext = Path::new(file).extension()?.to_str()?;
        let home = std::env::var("HOME").ok()?;
        let path = format!("{}/.config/catbath/syntax/{}", home, ext);
        let content = std::fs::read_to_string(path).ok()?;
        Some(Self::parse(&content))
    }

    fn parse(content: &str) -> Self {
        let mut kw = HashSet::new();
        let mut ty = HashSet::new();
        let mut cm = String::new();
        let mut st = '"';

        for line in content.lines() {
            if let Some(rest) = line.strip_prefix("keywords:") {
                kw.extend(rest.split_whitespace().map(String::from));
            } else if let Some(rest) = line.strip_prefix("types:") {
                ty.extend(rest.split_whitespace().map(String::from));
            } else if let Some(rest) = line.strip_prefix("comment:") {
                cm = rest.trim().to_string();
            } else if let Some(rest) = line.strip_prefix("string:") {
                st = rest.trim().chars().next().unwrap_or('"');
            }
        }
        Self {
            keywords: kw,
            types: ty,
            comment: cm,
            string: st,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Syntax;

    #[test]
    fn parses_config() {
        let syntax = Syntax::parse(
            "keywords: fn let mut\n\
             types: String usize\n\
             comment: //\n\
             string: '\n",
        );

        assert!(syntax.keywords.contains("fn"));
        assert!(syntax.types.contains("String"));
        assert_eq!(syntax.comment, "//");
        assert_eq!(syntax.string, '\'');
    }
}
