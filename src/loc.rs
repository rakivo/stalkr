use std::fmt;

pub struct Loc(pub String, pub usize, pub usize);

impl fmt::Display for Loc {
    fn fmt(&self, fm: &mut fmt::Formatter<'_>) -> fmt::Result {
        let Self(f, r, c) = self;
        write!(fm, "{f}:{r}:{c}")
    }
}

impl fmt::Debug for Loc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self, f)
    }
}

impl Loc {
    // O(log lines_count)
    #[inline]
    pub fn from_precomputed(
        line_starts: &[usize],
        match_byte_index: usize,
        path: String
    ) -> Self {
        let i = match line_starts.binary_search(&match_byte_index) {
            Ok(i)  => i,
            Err(i) => i
        };

        let row = i + 1;
        let col = match_byte_index - line_starts[i] + 1;

        Self(path, row, col)
    }

    // O(n)
    #[inline]
    pub fn precompute(h: &[u8]) -> Vec<usize> {
        let mut v = Vec::with_capacity(256);
        v.push(0);
        for (i, &b) in h.iter().enumerate() {
            if b == b'\n' { v.push(i + 1); }
        } v
    }
}

