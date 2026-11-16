//! One name of a whitelist entry, matched as Mole matches it with `[[ $path == $pattern ]]`: `*`
//! is any run of characters, `?` is one character, `[...]` is one character from a set, and `\`
//! takes the next character as written. Matching never asks the disk, so a name that does not
//! exist yet matches the same way.
use std::borrow::Cow;
use std::ffi::OsStr;
use std::fmt;
use unicode_normalization::UnicodeNormalization as _;
use unicode_normalization::char::canonical_combining_class;

/// Why a name cannot be matched the way Mole's shell would match it.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GlobError {
    UnknownClass(String),
    BackwardRange(char, char),
    /// `[=e=]` and `[.e.]` name what the shell's locale treats as one letter.
    Collation(String),
}

impl fmt::Display for GlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownClass(name) => write!(f, "[:{name}:] is not a character class"),
            Self::BackwardRange(lo, hi) => {
                write!(f, "the range {lo}-{hi} runs backwards and matches nothing")
            }
            Self::Collation(form) => write!(f, "{form} depends on the shell's locale"),
        }
    }
}

impl std::error::Error for GlobError {}

/// The key a name is compared under. On a volume that folds case it is the name in NFC, with
/// each character mapped to lowercase through its uppercase, one character for one. `É`, `é`
/// and `e` + U+0301 then share a key, and `?` still counts `ß` as one character. On other
/// volumes it is the name's bytes.
pub(crate) fn fold(name: &OsStr, fold_case: bool, out: &mut Vec<u8>) {
    out.clear();
    let bytes = name.as_encoded_bytes();
    if !fold_case {
        out.extend_from_slice(bytes);
    } else if bytes.is_ascii() {
        out.extend(bytes.iter().map(u8::to_ascii_lowercase));
    } else {
        let mut buf = [0; 4];
        for c in folded(&String::from_utf8_lossy(bytes)) {
            out.extend_from_slice(c.encode_utf8(&mut buf).as_bytes());
        }
    }
}

fn folded(text: &str) -> impl Iterator<Item = char> + '_ {
    text.nfd().map(fold_char).nfc()
}

fn fold_char(c: char) -> char {
    let upper = single(c.to_uppercase()).unwrap_or(c);
    single(upper.to_lowercase()).unwrap_or(upper)
}

fn single(mut mapped: impl Iterator<Item = char>) -> Option<char> {
    let first = mapped.next()?;
    mapped.next().is_none().then_some(first)
}

enum Token {
    Char(char),
    Star,
    One,
    Set(Set),
}

struct Set {
    negated: bool,
    items: Vec<Item>,
}

type Class = fn(char) -> bool;

enum Item {
    Char(char),
    Range(char, char),
    Class(Class),
}

/// The POSIX classes, read as a UTF-8 shell locale reads them.
const CLASSES: &[(&str, Class)] = &[
    ("alnum", char::is_alphanumeric),
    ("alpha", char::is_alphabetic),
    ("blank", |c| c == ' ' || c == '\t'),
    ("cntrl", char::is_control),
    ("digit", |c| c.is_ascii_digit()),
    ("graph", |c| !c.is_control() && !c.is_whitespace()),
    ("lower", char::is_lowercase),
    ("print", |c| !c.is_control()),
    ("punct", |c| {
        !c.is_alphanumeric() && !c.is_whitespace() && !c.is_control()
    }),
    ("space", char::is_whitespace),
    ("upper", char::is_uppercase),
    ("word", |c| c.is_alphanumeric() || c == '_'),
    ("xdigit", |c| c.is_ascii_hexdigit()),
];

/// A name with a wildcard or an escape in it.
pub(crate) struct Glob {
    tokens: Box<[Token]>,
    /// The name as written, folded. A folder named `[2020] Album` matches it, as it matches
    /// Mole's quoted comparison.
    written: Box<[u8]>,
    fold_case: bool,
}

impl Glob {
    /// `Ok(None)` for a name with nothing to expand, which matches by its [`fold`] key alone.
    pub(crate) fn parse(name: &OsStr, fold_case: bool) -> Result<Option<Self>, GlobError> {
        let Some(text) = name.to_str() else {
            return Ok(None);
        };
        if !text.contains(['*', '?', '[', '\\']) {
            return Ok(None);
        }
        let text: Cow<'_, str> = if fold_case {
            Cow::Owned(text.nfc().collect())
        } else {
            Cow::Borrowed(text)
        };
        let chars: Vec<char> = text.chars().collect();
        let mut tokens = Vec::new();
        let mut expands = false;
        let mut i = 0;
        while let Some(&c) = chars.get(i) {
            i += 1;
            match c {
                '*' => {
                    expands = true;
                    if !matches!(tokens.last(), Some(Token::Star)) {
                        tokens.push(Token::Star);
                    }
                }
                '?' => {
                    expands = true;
                    tokens.push(Token::One);
                }
                '\\' if i < chars.len() => {
                    expands = true;
                    tokens.push(Token::Char(chars[i]));
                    i += 1;
                }
                '[' => match Set::parse(&chars[i..])? {
                    Some((set, used)) => {
                        expands = true;
                        tokens.push(Token::Set(set));
                        i += used;
                    }
                    None => tokens.push(Token::Char('[')),
                },
                _ => tokens.push(Token::Char(c)),
            }
        }
        if !expands {
            return Ok(None);
        }
        let mut written = Vec::new();
        fold(name, fold_case, &mut written);
        Ok(Some(Self {
            tokens: fold_runs(tokens, fold_case),
            written: written.into(),
            fold_case,
        }))
    }

    pub(crate) fn same(&self, other: &Self) -> bool {
        self.written == other.written
    }

    /// `name` is a key from [`fold`].
    pub(crate) fn matches(&self, name: &[u8]) -> bool {
        *name == *self.written || self.matches_text(&String::from_utf8_lossy(name))
    }

    /// The shell's one-star backtrack: each token takes a fixed run, so on a mismatch only the
    /// last `*` takes one more character.
    fn matches_text(&self, name: &str) -> bool {
        let (mut t, mut n) = (0, 0);
        let mut retry: Option<(usize, usize)> = None;
        loop {
            match self.tokens.get(t) {
                Some(Token::Star) => {
                    t += 1;
                    retry = Some((t, n));
                    continue;
                }
                Some(token) => {
                    if let Some(used) = self.take(token, &name[n..]) {
                        t += 1;
                        n += used;
                        continue;
                    }
                }
                None if n == name.len() => return true,
                None => {}
            }
            let Some((after, from)) = retry else {
                return false;
            };
            let Some(skipped) = name[from..].chars().next() else {
                return false;
            };
            t = after;
            n = from + skipped.len_utf8();
            retry = Some((t, n));
        }
    }

    /// The bytes `token` takes from the front of `rest`, when it matches there.
    fn take(&self, token: &Token, rest: &str) -> Option<usize> {
        let first = rest.chars().next()?;
        match token {
            Token::Char(c) => (first == *c).then(|| c.len_utf8()),
            Token::One => Some(cluster(rest)),
            Token::Set(set) => set.has(first, self.fold_case).then(|| cluster(rest)),
            Token::Star => None,
        }
    }
}

/// Folds each run of plain characters the way [`fold`] folds a name.
fn fold_runs(tokens: Vec<Token>, fold_case: bool) -> Box<[Token]> {
    if !fold_case {
        return tokens.into();
    }
    let mut out = Vec::with_capacity(tokens.len());
    let mut run = String::new();
    for token in tokens {
        match token {
            Token::Char(c) => run.push(c),
            other => {
                out.extend(folded(&run).map(Token::Char));
                run.clear();
                out.push(other);
            }
        }
    }
    out.extend(folded(&run).map(Token::Char));
    out.into()
}

/// One character and the combining marks after it, which `?` and `[...]` take whole.
fn cluster(rest: &str) -> usize {
    rest.char_indices()
        .skip(1)
        .find(|&(_, c)| canonical_combining_class(c) == 0)
        .map_or(rest.len(), |(at, _)| at)
}

impl Set {
    /// Parses what follows a `[`, and how many characters the set used with its `]`. `Ok(None)`
    /// when no `]` closes it: the `[` is then a plain character, as it is in a shell.
    fn parse(chars: &[char]) -> Result<Option<(Self, usize)>, GlobError> {
        let negated = matches!(chars.first(), Some('!' | '^'));
        let open = usize::from(negated);
        let mut i = open;
        let mut items = Vec::new();
        loop {
            let Some(&c) = chars.get(i) else {
                return Ok(None);
            };
            if c == ']' && i > open {
                return Ok(Some((Self { negated, items }, i + 1)));
            }
            if c == '['
                && let Some(&kind @ (':' | '=' | '.')) = chars.get(i + 1)
                && let Some(len) = chars[i + 2..].windows(2).position(|w| w == [kind, ']'])
            {
                let name: String = chars[i + 2..i + 2 + len].iter().collect();
                if kind != ':' {
                    return Err(GlobError::Collation(format!("[{kind}{name}{kind}]")));
                }
                let Some(&(_, has)) = CLASSES.iter().find(|(known, _)| *known == name) else {
                    return Err(GlobError::UnknownClass(name));
                };
                items.push(Item::Class(has));
                i += len + 4;
                continue;
            }
            let (lo, used) = escaped(chars, i);
            i += used;
            if chars.get(i) == Some(&'-')
                && let Some(&next) = chars.get(i + 1)
                && next != ']'
            {
                let (hi, used) = escaped(chars, i + 1);
                if hi < lo {
                    return Err(GlobError::BackwardRange(lo, hi));
                }
                items.push(Item::Range(lo, hi));
                i += 1 + used;
            } else {
                items.push(Item::Char(lo));
            }
        }
    }

    /// A folded name has lost its case, so either case of `c` may be the one Mole saw. Taking
    /// both can only keep more.
    fn has(&self, c: char, fold_case: bool) -> bool {
        let inside = |x: char| self.items.iter().any(|item| item.has(x));
        let other = if fold_case {
            single(c.to_uppercase()).unwrap_or(c)
        } else {
            c
        };
        [c, other].into_iter().any(|x| inside(x) != self.negated)
    }
}

impl Item {
    fn has(&self, c: char) -> bool {
        match *self {
            Self::Char(m) => c == m,
            Self::Range(lo, hi) => (lo..=hi).contains(&c),
            Self::Class(has) => has(c),
        }
    }
}

/// The character at `i`, or the one after it when `i` holds a `\`, and how many it used.
fn escaped(chars: &[char], i: usize) -> (char, usize) {
    match chars.get(i + 1) {
        Some(&next) if chars[i] == '\\' => (next, 2),
        _ => (chars[i], 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glob(pattern: &str, fold_case: bool) -> Glob {
        Glob::parse(OsStr::new(pattern), fold_case)
            .unwrap()
            .unwrap_or_else(|| panic!("{pattern} has a wildcard"))
    }

    fn matches(pattern: &str, name: &str, fold_case: bool) -> bool {
        let mut key = Vec::new();
        fold(OsStr::new(name), fold_case, &mut key);
        glob(pattern, fold_case).matches(&key)
    }

    #[test]
    fn wildcards_match_as_in_a_shell() {
        for (pattern, name, want) in [
            ("com.foo.*", "com.foo.bar", true),
            ("com.foo.*", "com.foobar", false),
            ("*", "", true),
            ("a*b*c", "aXbYc", true),
            ("a*b*c", "aXbY", false),
            ("?ar", "bar", true),
            ("?ar", "ar", false),
            ("[abc]x", "bx", true),
            ("[!abc]x", "bx", false),
            ("[^abc]x", "dx", true),
            ("[a-c]x", "cx", true),
            ("[]]x", "]x", true),
            ("v[[:digit:]]", "v7", true),
            ("v[[:digit:]]", "vx", false),
            ("a\\*b", "a*b", true),
            ("a\\*b", "aXb", false),
        ] {
            let got = matches(pattern, name, false);
            assert_eq!(got, want, "{pattern} against {name}");
        }
    }

    #[test]
    fn a_bracket_with_no_close_is_a_plain_character() {
        assert!(Glob::parse(OsStr::new("x[y"), false).unwrap().is_none());
    }

    #[test]
    fn a_name_as_written_matches_even_when_its_brackets_would_not() {
        assert!(matches("[2020] Album", "[2020] Album", false));
        assert!(matches("[2020] Album", "2 Album", false));
        assert!(!matches("[2020] Album", "3 Album", false));
    }

    #[test]
    fn folding_ignores_case_and_normalization_form() {
        let key = |name: &str| {
            let mut out = Vec::new();
            fold(OsStr::new(name), true, &mut out);
            String::from_utf8(out).unwrap()
        };
        assert_eq!(key("Caf\u{e9}"), key("Cafe\u{301}"));
        assert_eq!(key("\u{c9}T\u{c9}"), key("\u{e9}t\u{e9}"));
        assert_eq!(key("Stra\u{df}e").chars().count(), 6);
        assert_ne!(key("Caf\u{e9}"), key("Cafe"));
        let mut raw = Vec::new();
        fold(OsStr::new("Caf\u{e9}"), false, &mut raw);
        assert_eq!(raw, "Caf\u{e9}".as_bytes());
    }

    #[test]
    fn a_folded_glob_matches_any_case_and_form() {
        assert!(matches("E\u{301}*", "\u{e9}t\u{e9}", true));
        assert!(matches("caf?", "Cafe\u{301}", true));
        assert!(matches("Stra?e", "STRA\u{df}E", true));
        assert!(matches("[\u{c9}]t\u{e9}", "\u{e9}t\u{e9}", true));
        assert!(matches("[A-C]x", "bx", true));
        assert!(matches("caf?", "cafe\u{301}", false));
        assert!(!matches("caf?", "Cafe\u{301}", false));
    }

    #[test]
    fn brackets_mole_would_read_another_way_are_errors() {
        for (pattern, want) in [
            ("[[:nope:]]", GlobError::UnknownClass("nope".into())),
            ("[z-a]", GlobError::BackwardRange('z', 'a')),
            ("[[=e=]]", GlobError::Collation("[=e=]".into())),
        ] {
            let got = Glob::parse(OsStr::new(pattern), false).err();
            assert_eq!(got, Some(want), "{pattern}");
        }
    }
}
