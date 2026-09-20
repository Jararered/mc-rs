/// Java's 48-bit linear congruential RNG, as used by Beta world generation.
pub(super) struct JavaRandom {
    state: u64,
}

impl JavaRandom {
    const MULTIPLIER: u64 = 0x5deece66d;
    const ADDEND: u64 = 0xb;
    const MASK: u64 = (1 << 48) - 1;

    pub fn new(seed: u64) -> Self {
        Self {
            state: (seed ^ Self::MULTIPLIER) & Self::MASK,
        }
    }

    pub fn next_bits(&mut self, bits: u32) -> u32 {
        self.state = self
            .state
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(Self::ADDEND)
            & Self::MASK;
        (self.state >> (48 - bits)) as u32
    }

    pub fn next_int(&mut self, bound: u32) -> u32 {
        assert!(bound > 0 && bound <= i32::MAX as u32);
        if bound.is_power_of_two() {
            return ((bound as u64 * self.next_bits(31) as u64) >> 31) as u32;
        }
        loop {
            let bits = self.next_bits(31);
            let value = bits % bound;
            if (bits as i32)
                .wrapping_sub(value as i32)
                .wrapping_add(bound as i32 - 1)
                >= 0
            {
                return value;
            }
        }
    }

    pub fn next_double(&mut self) -> f64 {
        let bits = ((self.next_bits(26) as u64) << 27) | self.next_bits(27) as u64;
        bits as f64 / (1u64 << 53) as f64
    }
}
