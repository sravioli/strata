//! Feature probes: small functions that use the Rust features we suspect are
//! a problem for Charon/Aeneas. Not production code. See spike/README.md for
//! what was observed for each one.

// --- integer overflow ---

/// Plain `+`: panics on overflow in debug builds.
pub fn next_number_plain(latest: u32) -> u32 {
    latest + 1
}

/// `checked_add`: no panic.
pub fn next_number_checked(latest: u32) -> Option<u32> {
    latest.checked_add(1)
}

// --- generics + trait ---

pub trait HasNumber {
    fn number(&self) -> u32;
}

pub struct Rev {
    pub number: u32,
}

impl HasNumber for Rev {
    fn number(&self) -> u32 {
        self.number
    }
}

pub fn is_newer<T: HasNumber>(a: &T, b: &T) -> bool {
    a.number() > b.number()
}

// --- dyn ---

pub fn number_dyn(x: &dyn HasNumber) -> u32 {
    x.number()
}

// --- closure ---

pub fn apply_closure(n: u32) -> bool {
    let newer = |a: u32| a < n;
    newer(3)
}

// --- loop over a slice (index loop) ---

pub fn max_number_loop(xs: &[u32]) -> u32 {
    let mut m = 0;
    let mut i = 0;
    while i < xs.len() {
        if xs[i] > m {
            m = xs[i];
        }
        i += 1;
    }
    m
}

// --- iterator chain ---

pub fn max_number_iter(xs: &[u32]) -> u32 {
    xs.iter().copied().filter(|x| *x > 0).max().unwrap_or(0)
}

// --- for loop over a Vec ---

pub fn count_ready(xs: &Vec<bool>) -> usize {
    let mut n = 0;
    for x in xs {
        if *x {
            n += 1;
        }
    }
    n
}

// --- strings ---

pub fn is_blank(s: &str) -> bool {
    s.is_empty()
}

pub fn make_title(s: &str) -> String {
    let mut t = String::from("doc: ");
    t.push_str(s);
    t
}

// --- Result + `?` ---

pub enum DomainError {
    Overflow,
}

pub fn next_number_result(latest: u32) -> Result<u32, DomainError> {
    let n = latest.checked_add(1).ok_or(DomainError::Overflow)?;
    Ok(n)
}
