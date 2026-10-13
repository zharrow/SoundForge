//! Générateur pseudo-aléatoire déterministe (xorshift64*).
//!
//! Une même graine produit toujours le même son : indispensable pour
//! régénérer un SFX à l'identique ou reproduire une variante.

#[derive(Debug, Clone)]
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    /// Valeur uniforme dans [0, 1).
    pub fn unit(&mut self) -> f32 {
        (self.next_u64() >> 40) as f32 / (1u64 << 24) as f32
    }

    /// Valeur uniforme dans [-1, 1).
    pub fn bipolar(&mut self) -> f32 {
        self.unit() * 2.0 - 1.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_seed_gives_same_sequence() {
        let (mut a, mut b) = (Rng::new(42), Rng::new(42));
        assert!((0..100).all(|_| a.next_u64() == b.next_u64()));
    }

    #[test]
    fn unit_stays_in_range() {
        let mut rng = Rng::new(7);
        assert!(
            (0..10_000)
                .map(|_| rng.unit())
                .all(|v| (0.0..1.0).contains(&v))
        );
    }
}
