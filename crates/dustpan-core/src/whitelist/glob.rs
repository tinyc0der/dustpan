use caseless::Caseless as _;
use std::borrow::Cow;
use std::cell::OnceCell;
use std::ffi::OsStr;
use std::fmt;
use unicode_normalization::UnicodeNormalization as _;
use unicode_normalization::char::canonical_combining_class;

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum GlobError {
    UnknownClass(String),
    BackwardRange(String, String),
    /// `[=ab=]` or `[.ab.]`: the C locale Mole matches under has no element of more than one
    /// character.
    Collation(String),
}

impl fmt::Display for GlobError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnknownClass(name) => write!(f, "[:{name}:] is not a character class"),
            Self::BackwardRange(lo, hi) => {
                write!(f, "the range {lo}-{hi} runs backwards and matches nothing")
            }
            Self::Collation(form) => write!(f, "{form} names no single character"),
        }
    }
}

impl std::error::Error for GlobError {}

/// On a volume that folds case, the key is NFC of the NFD form under full case folding
/// (`CaseFolding.txt` status C and F), so `ß` and `ﬁ` meet `ss` and `fi`. Each folded character
/// then goes through the lowercase of its uppercase where each map gives one character, which
/// joins `ı` with `i` and keeps more than the volume does. Canonically equivalent names share a
/// key and `é` stays one character.
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
    text.nfd().default_case_fold().nfd().map(fold_char).nfc()
}

/// The key before full case folding: one character in, one out, so `?` counts `ß` as one.
fn one_for_one(text: &str) -> impl Iterator<Item = char> + '_ {
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

/// One name a glob is asked about: the bytes the disk gave, and its [`fold`] key.
pub(crate) struct Subject<'a> {
    raw: &'a [u8],
    key: &'a [u8],
    chars: OnceCell<Vec<char>>,
    one_for_one: OnceCell<Vec<char>>,
    decomposed: OnceCell<Vec<char>>,
    hangul: OnceCell<Option<Vec<char>>>,
}

impl<'a> Subject<'a> {
    pub(crate) fn new(raw: &'a OsStr, key: &'a [u8]) -> Self {
        Self {
            raw: raw.as_encoded_bytes(),
            key,
            chars: OnceCell::new(),
            one_for_one: OnceCell::new(),
            decomposed: OnceCell::new(),
            hangul: OnceCell::new(),
        }
    }

    pub(crate) fn key(&self) -> &[u8] {
        self.key
    }

    fn key_chars(&self) -> &[char] {
        self.chars
            .get_or_init(|| String::from_utf8_lossy(self.key).chars().collect())
    }

    fn one_for_one_chars(&self) -> &[char] {
        self.one_for_one
            .get_or_init(|| one_for_one(&String::from_utf8_lossy(self.raw)).collect())
    }

    fn decomposed_chars(&self) -> &[char] {
        self.decomposed
            .get_or_init(|| String::from_utf8_lossy(self.key).nfd().collect())
    }

    /// The key with each Hangul syllable that ends in a final consonant split into the
    /// syllable without it and that consonant: U+AC01 `각` becomes U+AC00 `가` and U+11A8 `ᆨ`,
    /// which is canonically the same. A precomposed syllable in a set can stop there, where NFD
    /// would take it apart. `None` when the key has no such syllable.
    fn hangul_split_chars(&self) -> Option<&[char]> {
        const SYLLABLES: std::ops::RangeInclusive<u32> = 0xAC00..=0xD7A3;
        const FINALS: u32 = 28;
        const NO_FINAL: u32 = 0x11A7;
        self.hangul
            .get_or_init(|| {
                let key = self.key_chars();
                let finals = |c: char| {
                    let n = u32::from(c);
                    let t = if SYLLABLES.contains(&n) {
                        (n - 0xAC00) % FINALS
                    } else {
                        0
                    };
                    (t != 0).then(|| (char::from_u32(n - t), char::from_u32(NO_FINAL + t)))
                };
                key.iter().any(|&c| finals(c).is_some()).then(|| {
                    key.iter()
                        .flat_map(|&c| match finals(c) {
                            Some((Some(lv), Some(t))) => vec![lv, t],
                            _ => vec![c],
                        })
                        .collect()
                })
            })
            .as_deref()
    }
}

/// What a pattern reads one at a time: a byte, as Mole's `LC_ALL=C` shell does, or a
/// character of a fold key.
trait Unit: Copy + PartialOrd {
    fn ascii(self) -> Option<u8>;
    fn text(units: &[Self]) -> String;
    fn class(name: &str) -> Option<fn(Self) -> bool>;
    fn is_mark(self) -> bool;
    fn other_case(self) -> Self;
}

type ByteClass = fn(u8) -> bool;
type CharClass = fn(char) -> bool;

const BYTE_CLASSES: &[(&str, ByteClass)] = &[
    ("alnum", |b| b.is_ascii_alphanumeric()),
    ("alpha", |b| b.is_ascii_alphabetic()),
    ("ascii", |b| b.is_ascii()),
    ("blank", |b| b == b' ' || b == b'\t'),
    ("cntrl", |b| b.is_ascii_control()),
    ("digit", |b| b.is_ascii_digit()),
    ("graph", |b| b.is_ascii_graphic()),
    ("lower", |b| b.is_ascii_lowercase()),
    ("print", |b| b.is_ascii_graphic() || b == b' '),
    ("punct", |b| b.is_ascii_punctuation()),
    ("space", |b| b.is_ascii_whitespace() || b == 0x0b),
    ("upper", |b| b.is_ascii_uppercase()),
    ("word", |b| b.is_ascii_alphanumeric() || b == b'_'),
    ("xdigit", |b| b.is_ascii_hexdigit()),
];

const CHAR_CLASSES: &[(&str, CharClass)] = &[
    ("alnum", char::is_alphanumeric),
    ("alpha", char::is_alphabetic),
    ("ascii", |c| c.is_ascii()),
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

impl Unit for u8 {
    fn ascii(self) -> Option<u8> {
        self.is_ascii().then_some(self)
    }

    fn text(units: &[Self]) -> String {
        String::from_utf8_lossy(units).into_owned()
    }

    fn class(name: &str) -> Option<ByteClass> {
        BYTE_CLASSES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, c)| c)
    }

    fn is_mark(self) -> bool {
        false
    }

    fn other_case(self) -> Self {
        self
    }
}

impl Unit for char {
    fn ascii(self) -> Option<u8> {
        u8::try_from(self).ok().filter(u8::is_ascii)
    }

    fn text(units: &[Self]) -> String {
        units.iter().collect()
    }

    fn class(name: &str) -> Option<CharClass> {
        CHAR_CLASSES
            .iter()
            .find(|(n, _)| *n == name)
            .map(|&(_, c)| c)
    }

    fn is_mark(self) -> bool {
        canonical_combining_class(self) != 0
    }

    fn other_case(self) -> Self {
        single(self.to_uppercase()).unwrap_or(self)
    }
}

enum Token<U: Unit> {
    Unit(U),
    Star,
    One,
    Set(Set<U>),
}

struct Set<U: Unit> {
    negated: bool,
    items: Vec<Item<U>>,
}

enum Item<U: Unit> {
    Unit(U),
    Range(U, U),
    Class(fn(U) -> bool),
}

struct Pattern<U: Unit> {
    tokens: Box<[Token<U>]>,
    /// A folded key no longer says which case Mole would have seen, so a set tries both.
    either_case: bool,
}

pub(crate) struct Glob {
    bytes: Pattern<u8>,
    chars: Pattern<char>,
    /// The pattern over [`one_for_one`] characters, for a name with a character outside ASCII.
    one_for_one: Option<Pattern<char>>,
    /// The pattern over the key's NFD, for a pattern that spells a character outside ASCII
    /// right after a wildcard. NFC can compose that character into the one the wildcard takes:
    /// `x` and U+0308 are U+1E8D, so `?\u{308}` would find no mark to match.
    decomposed: Option<Pattern<char>>,
    /// A folder named `[2020] Album` matches this, as it matches Mole's quoted comparison.
    written: Box<[u8]>,
}

pub(crate) enum Name {
    Plain,
    Glob(Glob),
}

impl Glob {
    /// A name matches as Mole matches it, by its bytes, or by its key: as written, or with
    /// each wildcard taking characters. The key only ever adds names.
    pub(crate) fn parse(name: &OsStr, fold_case: bool) -> Result<Name, GlobError> {
        let raw = name.as_encoded_bytes();
        if !raw.iter().any(|b| b"*?[\\".contains(b)) {
            return Ok(Name::Plain);
        }
        let Some(bytes) = Pattern::parse(raw, false)? else {
            return Ok(Name::Plain);
        };
        let text = String::from_utf8_lossy(raw);
        let text: Cow<'_, str> = if fold_case {
            Cow::Owned(text.nfc().collect())
        } else {
            text
        };
        let units: Vec<char> = text.chars().collect();
        let Some(mut chars) = Pattern::parse(&units, fold_case)? else {
            return Ok(Name::Plain);
        };
        let mut one_for_one = None;
        let mut decomposed = None;
        if fold_case {
            one_for_one = Pattern::parse(&units, true)?.map(|mut p| {
                p.tokens = fold_runs(p.tokens, one_for_one_run);
                p
            });
            decomposed = Pattern::parse(&units, true)?
                .map(|mut p| {
                    p.tokens = fold_runs(p.tokens, decomposed_run);
                    p
                })
                .filter(Pattern::spells_after_a_wildcard);
            chars.tokens = fold_runs(chars.tokens, folded_run);
        }
        let mut written = Vec::new();
        fold(name, fold_case, &mut written);
        Ok(Name::Glob(Self {
            bytes,
            chars,
            one_for_one,
            decomposed,
            written: written.into(),
        }))
    }

    pub(crate) fn same(&self, other: &Self) -> bool {
        self.written == other.written
    }

    pub(crate) fn matches(&self, name: &Subject<'_>) -> bool {
        *name.key == *self.written
            || self.bytes.matches(name.raw)
            || self.chars.matches(name.key_chars())
            || (!name.raw.is_ascii()
                && (self
                    .one_for_one
                    .as_ref()
                    .is_some_and(|p| p.matches(name.one_for_one_chars()))
                    || self
                        .decomposed
                        .as_ref()
                        .is_some_and(|p| p.matches(name.decomposed_chars()))
                    || name
                        .hangul_split_chars()
                        .is_some_and(|split| self.chars.matches(split))))
    }
}

fn folded_run(run: &str) -> Vec<char> {
    folded(run).collect()
}

fn decomposed_run(run: &str) -> Vec<char> {
    folded(run).nfd().collect()
}

fn one_for_one_run(run: &str) -> Vec<char> {
    one_for_one(run).collect()
}

fn fold_runs(tokens: Box<[Token<char>]>, fold: fn(&str) -> Vec<char>) -> Box<[Token<char>]> {
    let mut out = Vec::with_capacity(tokens.len());
    let mut run = String::new();
    for token in tokens {
        match token {
            Token::Unit(c) => run.push(c),
            other => {
                out.extend(fold(&run).into_iter().map(Token::Unit));
                run.clear();
                out.push(other);
            }
        }
    }
    out.extend(fold(&run).into_iter().map(Token::Unit));
    out.into()
}

impl<U: Unit> Pattern<U> {
    fn parse(units: &[U], either_case: bool) -> Result<Option<Self>, GlobError> {
        let mut tokens = Vec::new();
        let mut expands = false;
        let mut i = 0;
        while let Some(&u) = units.get(i) {
            i += 1;
            match u.ascii() {
                Some(b'*') => {
                    expands = true;
                    if !matches!(tokens.last(), Some(Token::Star)) {
                        tokens.push(Token::Star);
                    }
                }
                Some(b'?') => {
                    expands = true;
                    tokens.push(Token::One);
                }
                Some(b'\\') if i < units.len() => {
                    expands = true;
                    tokens.push(Token::Unit(units[i]));
                    i += 1;
                }
                Some(b'[') => match Set::parse(&units[i..])? {
                    Some((set, used)) => {
                        expands = true;
                        tokens.push(Token::Set(set));
                        i += used;
                    }
                    None => tokens.push(Token::Unit(u)),
                },
                _ => tokens.push(Token::Unit(u)),
            }
        }
        Ok(expands.then(|| Self {
            tokens: tokens.into(),
            either_case,
        }))
    }

    fn matches(&self, name: &[U]) -> bool {
        let (mut t, mut n) = (0, 0);
        let mut last_star: Option<(usize, usize)> = None;
        loop {
            match self.tokens.get(t) {
                Some(Token::Star) => {
                    t += 1;
                    last_star = Some((t, n));
                    continue;
                }
                Some(_) => {
                    if let Some(used) = self.take(t, &name[n..]) {
                        t += 1;
                        n += used;
                        continue;
                    }
                }
                None if n == name.len() => return true,
                None => {}
            }
            match last_star {
                Some((after, from)) if from < name.len() => {
                    t = after;
                    n = from + 1;
                    last_star = Some((t, n));
                }
                _ => return false,
            }
        }
    }

    fn spells_after_a_wildcard(&self) -> bool {
        self.tokens.windows(2).any(|pair| {
            !matches!(pair[0], Token::Unit(_))
                && matches!(pair[1], Token::Unit(u) if u.ascii().is_none())
        })
    }

    fn take(&self, t: usize, rest: &[U]) -> Option<usize> {
        let &first = rest.first()?;
        let admitted = match &self.tokens[t] {
            Token::Unit(u) => return (first == *u).then_some(1),
            Token::One => true,
            Token::Set(set) => set.admits(first, self.either_case),
            Token::Star => return None,
        };
        admitted.then(|| self.width(t, rest))
    }

    /// `?` and `[...]` take a character with the combining marks after it, so a key that NFC
    /// could not compose still counts one letter as one. When the pattern spells a mark next,
    /// past any `*`, the wildcard takes the character alone and leaves the marks to the
    /// pattern: `a?\u{300}t` keeps `a_\u{300}t`.
    fn width(&self, t: usize, rest: &[U]) -> usize {
        let spells_a_mark = self.tokens[t + 1..]
            .iter()
            .find(|token| !matches!(token, Token::Star))
            .is_some_and(|token| matches!(token, Token::Unit(u) if u.is_mark()));
        if spells_a_mark {
            1
        } else {
            1 + rest[1..].iter().take_while(|u| u.is_mark()).count()
        }
    }
}

impl<U: Unit> Set<U> {
    /// Reads what follows a `[`. With no `]` to close it, the `[` is a plain character, as in a
    /// shell.
    fn parse(units: &[U]) -> Result<Option<(Self, usize)>, GlobError> {
        let negated = matches!(units.first().and_then(|u| u.ascii()), Some(b'!' | b'^'));
        let open = usize::from(negated);
        let mut i = open;
        let mut items = Vec::new();
        loop {
            let Some(&u) = units.get(i) else {
                return Ok(None);
            };
            if u.ascii() == Some(b']') && i > open {
                return Ok(Some((Self { negated, items }, i + 1)));
            }
            if u.ascii() == Some(b'[')
                && let Some(kind @ (b':' | b'=' | b'.')) = units.get(i + 1).and_then(|u| u.ascii())
                && let Some(len) = units[i + 2..]
                    .windows(2)
                    .position(|w| w[0].ascii() == Some(kind) && w[1].ascii() == Some(b']'))
            {
                let inner = &units[i + 2..i + 2 + len];
                let name = U::text(inner);
                items.push(match (kind, inner) {
                    (b':', _) => Item::Class(U::class(&name).ok_or(GlobError::UnknownClass(name))?),
                    (_, &[one]) => Item::Unit(one),
                    _ => {
                        let kind = char::from(kind);
                        return Err(GlobError::Collation(format!("[{kind}{name}{kind}]")));
                    }
                });
                i += len + 4;
                continue;
            }
            let (lo, used) = unescaped(units, i);
            i += used;
            if units.get(i).and_then(|u| u.ascii()) == Some(b'-')
                && let Some(&next) = units.get(i + 1)
                && next.ascii() != Some(b']')
            {
                let (hi, used) = unescaped(units, i + 1);
                if hi < lo {
                    return Err(GlobError::BackwardRange(U::text(&[lo]), U::text(&[hi])));
                }
                items.push(Item::Range(lo, hi));
                i += 1 + used;
            } else {
                items.push(Item::Unit(lo));
            }
        }
    }

    fn admits(&self, u: U, either_case: bool) -> bool {
        let inside = |x: U| self.items.iter().any(|item| item.has(x));
        let other = if either_case { u.other_case() } else { u };
        [u, other].into_iter().any(|x| inside(x) != self.negated)
    }
}

impl<U: Unit> Item<U> {
    fn has(&self, u: U) -> bool {
        match *self {
            Self::Unit(m) => u == m,
            Self::Range(lo, hi) => lo <= u && u <= hi,
            Self::Class(has) => has(u),
        }
    }
}

/// The unit at `i`, or the one after it when `i` holds a `\`, and how many units that used.
fn unescaped<U: Unit>(units: &[U], i: usize) -> (U, usize) {
    match units.get(i + 1) {
        Some(&next) if units[i].ascii() == Some(b'\\') => (next, 2),
        _ => (units[i], 1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn glob(pattern: &str, fold_case: bool) -> Glob {
        match Glob::parse(OsStr::new(pattern), fold_case).unwrap() {
            Name::Glob(glob) => glob,
            Name::Plain => panic!("{pattern} has a wildcard"),
        }
    }

    fn matches(pattern: &str, name: &str, fold_case: bool) -> bool {
        let mut key = Vec::new();
        fold(OsStr::new(name), fold_case, &mut key);
        glob(pattern, fold_case).matches(&Subject::new(OsStr::new(name), &key))
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
            ("[[=e=]]x", "ex", true),
            ("[[.e.]]x", "ex", true),
            ("a\\*b", "a*b", true),
            ("a\\*b", "aXb", false),
        ] {
            let got = matches(pattern, name, false);
            assert_eq!(got, want, "{pattern} against {name}");
        }
    }

    #[test]
    fn a_bracket_with_no_close_is_a_plain_character() {
        assert!(matches!(
            Glob::parse(OsStr::new("x[y"), false),
            Ok(Name::Plain)
        ));
    }

    #[test]
    fn a_name_as_written_matches_even_when_its_brackets_would_not() {
        assert!(matches("[2020] Album", "[2020] Album", false));
        assert!(matches("[2020] Album", "2 Album", false));
        assert!(!matches("[2020] Album", "3 Album", false));
    }

    #[test]
    fn wildcards_take_bytes_as_mole_does_and_characters_as_the_key_does() {
        assert!(matches("caf??", "caf\u{e9}", false));
        assert!(matches("caf?", "caf\u{e9}", false));
        assert!(matches("v[![:alpha:]]*", "v\u{e9}", true));
        assert!(matches("caf?", "cafe\u{301}", false));
        assert!(!matches("caf???", "caf\u{e9}", false));
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
        assert_eq!(key("\u{3a3}"), key("\u{3c2}"));
        assert_eq!(key("Stra\u{df}e"), key("STRASSE"));
        assert_eq!(key("\u{fb03}"), "ffi");
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
        assert!(!matches("caf?", "Cafe\u{301}", false));
    }

    #[test]
    fn brackets_mole_would_read_another_way_are_errors() {
        let text = |s: &str| s.to_owned();
        for (pattern, want) in [
            ("[[:nope:]]", GlobError::UnknownClass(text("nope"))),
            ("[z-a]", GlobError::BackwardRange(text("z"), text("a"))),
            ("[[=ab=]]", GlobError::Collation(text("[=ab=]"))),
        ] {
            let got = Glob::parse(OsStr::new(pattern), false).err();
            assert_eq!(got, Some(want), "{pattern}");
        }
    }
}
