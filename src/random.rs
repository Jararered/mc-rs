/// Java's 48-bit linear congruential RNG, matching `java.util.Random`.
///
/// Used for deterministic world generation, star placement, item pile offsets,
/// and any other simulation code that must produce the same sequence as Beta.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub struct JavaRandom {
    state: u64,
}

impl JavaRandom {
    const MULTIPLIER: u64 = 0x0005_DEEC_E66D;
    const ADDEND: u64 = 0xb;
    const MASK: u64 = (1 << 48) - 1;

    /// Creates a new RNG seeded with `seed`, following Java's `new Random(seed)`.
    pub fn new(seed: u64) -> Self {
        Self {
            state: (seed ^ Self::MULTIPLIER) & Self::MASK,
        }
    }

    /// Restores an RNG from a previously captured state without re-mixing the seed.
    pub fn from_state(state: u64) -> Self {
        Self { state }
    }

    /// Returns the current internal state (the raw 48-bit value).
    pub fn state(&self) -> u64 {
        self.state
    }

    /// Advances the LCG and returns the next `bits` (1..=32) as a `u32`.
    pub fn next_bits(&mut self, bits: u32) -> u32 {
        self.state = self
            .state
            .wrapping_mul(Self::MULTIPLIER)
            .wrapping_add(Self::ADDEND)
            & Self::MASK;
        (self.state >> (48 - bits)) as u32
    }

    /// Returns a uniformly distributed integer in `[0, bound)`.
    ///
    /// Uses rejection sampling when `bound` is not a power of two, matching Java.
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

    /// Returns a uniformly distributed `f64` in `[0.0, 1.0)`.
    pub fn next_double(&mut self) -> f64 {
        let bits = ((self.next_bits(26) as u64) << 27) | self.next_bits(27) as u64;
        bits as f64 / (1u64 << 53) as f64
    }

    /// Returns a uniformly distributed `f32` in `[0.0, 1.0)`.
    pub fn next_float(&mut self) -> f32 {
        self.next_bits(24) as f32 / (1u32 << 24) as f32
    }

    /// Java's `nextLong`, which sign-extends each 32-bit half before combining.
    pub fn next_long(&mut self) -> i64 {
        let high = self.next_bits(32) as i32 as i64;
        let low = self.next_bits(32) as i32 as i64;
        (high << 32).wrapping_add(low)
    }
}

/// Java's `String.hashCode()`: `h = h * 31 + c`, wrapping in `i32` so that long
/// seeds overflow into the negative range exactly as they do in Java.
fn java_string_hash(text: &str) -> i32 {
    let mut hash: i32 = 0;
    for unit in text.encode_utf16() {
        hash = hash.wrapping_mul(31).wrapping_add(i32::from(unit));
    }
    hash
}

/// Parse a text seed into the 64-bit value world generation takes.
///
/// `GuiCreateWorld.actionPerformed`: text which `Long.parseLong` accepts is used
/// directly, so a seed copied out of `level.json` round-trips unchanged. Anything
/// else falls back to `String.hashCode()`, sign-extended from `int` to `long` as
/// the cast in the reference does. That sign extension is load-bearing: the
/// generator masks to 48 bits, so bits 32 through 47 decide the world for any
/// negative text seed.
///
/// The reference does not trim, and `Long.parseLong` rejects surrounding
/// whitespace, so `" 42 "` hashes as text rather than parsing as a number.
pub fn parse_seed(text: &str) -> u64 {
    match text.parse::<i64>() {
        Ok(value) => value.cast_unsigned(),
        Err(_) => i64::from(java_string_hash(text)).cast_unsigned(),
    }
}

/// Spawn-time randomness. Each caller keeps its own so tests do not need the resource.
#[derive(Clone, Debug)]
pub struct ItemRng {
    state: u64,
}

impl Default for ItemRng {
    fn default() -> Self {
        Self {
            state: 0x1234_5678_9ABC_DEF0,
        }
    }
}

impl ItemRng {
    /// Returns a uniform `f32` in `[0, 1)`.
    pub fn unit(&mut self) -> f32 {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (self.state >> 33) as f32 / (1u32 << 31) as f32
    }

    /// Returns a pseudo-random `u64`.
    pub fn next_u64(&mut self) -> u64 {
        let hi = self.unit().to_bits() as u64;
        let lo = self.unit().to_bits() as u64;
        hi << 32 | lo
    }
}
