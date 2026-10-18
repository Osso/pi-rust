/// Deterministic xorshift generator for differential corpora.
pub(crate) struct Corpus {
    state: u64,
}

impl Corpus {
    pub(crate) fn new(seed: u64) -> Self {
        Self { state: seed.max(1) }
    }

    pub(crate) fn next(&mut self) -> u64 {
        self.state ^= self.state << 13;
        self.state ^= self.state >> 7;
        self.state ^= self.state << 17;
        self.state
    }

    pub(crate) fn below(&mut self, bound: usize) -> usize {
        (self.next() % bound as u64) as usize
    }

    /// Picks `length` items from `alphabet` and concatenates them.
    pub(crate) fn text(&mut self, alphabet: &[&str], length: usize) -> String {
        (0..length).map(|_| alphabet[self.below(alphabet.len())]).collect()
    }
}
