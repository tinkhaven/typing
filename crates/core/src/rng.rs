//! A small, explicitly-specified PRNG.
//!
//! Exercise text is generated on the client from a seed the server issued. The
//! server then regenerates the same text from the same seed to check the
//! reported result against what was actually presented. That only works if both
//! sides agree bit-for-bit, so the generator is spelled out here rather than
//! taken from a crate whose algorithm may change between versions.
//!
//! This is PCG-XSH-RR 64/32 (O'Neill, 2014). It is not cryptographic.

/// Deterministic 32-bit random number generator.
#[derive(Clone, Debug)]
pub struct Rng {
    state: u64,
    inc: u64,
}

const MULT: u64 = 6_364_136_223_846_793_005;

impl Rng {
    /// Creates a generator from a seed. Every seed yields a distinct stream.
    pub fn from_seed(seed: u64) -> Self {
        let mut rng = Rng {
            state: 0,
            inc: (seed << 1) | 1,
        };
        rng.next_u32();
        rng.state = rng.state.wrapping_add(seed);
        rng.next_u32();
        rng
    }

    /// Returns the next value in the stream.
    pub fn next_u32(&mut self) -> u32 {
        let old = self.state;
        self.state = old.wrapping_mul(MULT).wrapping_add(self.inc);
        let xorshifted = (((old >> 18) ^ old) >> 27) as u32;
        let rot = (old >> 59) as u32;
        xorshifted.rotate_right(rot)
    }

    /// Returns a value in `0..n`, without the modulo bias of `next_u32() % n`.
    ///
    /// # Panics
    /// Panics if `n` is zero.
    pub fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0, "Rng::below requires a non-zero bound");
        // 2^32 % n, computed without 64-bit intermediates.
        let threshold = n.wrapping_neg() % n;
        loop {
            let r = self.next_u32();
            if r >= threshold {
                return r % n;
            }
        }
    }

    /// Picks an element uniformly, or `None` if the slice is empty.
    pub fn choose<'a, T>(&mut self, items: &'a [T]) -> Option<&'a T> {
        if items.is_empty() {
            return None;
        }
        items.get(self.below(items.len() as u32) as usize)
    }

    /// True with probability `1 - 1/n`, mirroring Klavaro's `rand() % n` idiom.
    ///
    /// Upstream writes `if (rand() % 15)` to mean "usually", the branch being
    /// taken whenever the remainder is non-zero. Naming it keeps the generators
    /// readable while preserving the exact probabilities.
    pub fn usually(&mut self, n: u32) -> bool {
        self.below(n) != 0
    }
}

#[cfg(test)]
mod tests {
    use super::Rng;

    #[test]
    fn is_deterministic_for_a_seed() {
        let a: Vec<u32> = (0..64).map(|_| Rng::from_seed(7).next_u32()).collect();
        assert!(a.windows(2).all(|w| w[0] == w[1]), "same seed must replay");

        let mut x = Rng::from_seed(7);
        let mut y = Rng::from_seed(7);
        let xs: Vec<u32> = (0..256).map(|_| x.next_u32()).collect();
        let ys: Vec<u32> = (0..256).map(|_| y.next_u32()).collect();
        assert_eq!(xs, ys);
    }

    #[test]
    fn different_seeds_diverge() {
        let mut x = Rng::from_seed(1);
        let mut y = Rng::from_seed(2);
        let xs: Vec<u32> = (0..32).map(|_| x.next_u32()).collect();
        let ys: Vec<u32> = (0..32).map(|_| y.next_u32()).collect();
        assert_ne!(xs, ys);
    }

    #[test]
    fn below_stays_in_range_and_covers_it() {
        let mut rng = Rng::from_seed(42);
        let mut seen = [false; 7];
        for _ in 0..2000 {
            let v = rng.below(7);
            assert!(v < 7);
            seen[v as usize] = true;
        }
        assert!(seen.iter().all(|&s| s), "every value in 0..7 should appear");
    }

    #[test]
    fn below_one_is_always_zero() {
        let mut rng = Rng::from_seed(3);
        assert!((0..100).all(|_| rng.below(1) == 0));
    }
}
