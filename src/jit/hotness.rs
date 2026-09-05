/// Lightweight execution counter used to promote cold code.
#[derive(Debug, Default)]
pub struct Hotness {
    hits: u32,
}

impl Hotness {
    #[inline]
    pub fn hit(&mut self) -> u32 {
        self.hits = self.hits.saturating_add(1);
        self.hits
    }

    #[inline]
    pub fn is_hot(&self, threshold: u32) -> bool {
        self.hits >= threshold
    }
}
