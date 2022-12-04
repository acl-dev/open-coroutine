#[cfg(target_pointer_width = "64")]
type DoubleUsize = u128;
#[cfg(target_pointer_width = "32")]
type DoubleUsize = u64;

/// Wyrand RNG.
#[repr(C)]
#[derive(Debug)]
pub struct Rng {
    pub(crate) state: u64,
}

impl Rng {
    pub fn gen_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0xA0761D6478BD642F);
        let t = u128::from(self.state) * u128::from(self.state ^ 0xE7037ED1A0B428DB);
        (t >> 64) as u64 ^ t as u64
    }

    pub fn gen_usize(&mut self) -> usize {
        self.gen_u64() as usize
    }

    pub fn gen_usize_to(&mut self, to: usize) -> usize {
        // Adapted from https://www.pcg-random.org/posts/bounded-rands.html
        const USIZE_BITS: usize = std::mem::size_of::<usize>() * 8;

        let mut x = self.gen_usize();
        let mut m = ((x as DoubleUsize * to as DoubleUsize) >> USIZE_BITS) as usize;
        let mut l = x.wrapping_mul(to);
        if l < to {
            let t = to.wrapping_neg() % to;
            while l < t {
                x = self.gen_usize();
                m = ((x as DoubleUsize * to as DoubleUsize) >> USIZE_BITS) as usize;
                l = x.wrapping_mul(to);
            }
        }
        m
    }
}

#[cfg(test)]
mod tests {
    use crate::random::Rng;
    use std::collections::HashSet;

    #[test]
    fn rng() {
        let mut rng = Rng { state: 3493858 };

        let mut remaining: HashSet<_> = (0..15).collect();

        while !remaining.is_empty() {
            let value = rng.gen_usize_to(15);
            assert!(value < 15, "{} is not less than 15!", value);
            remaining.remove(&value);
        }
    }
}
