use crate::{CancelPoint, SpectrumError};

#[derive(Clone, Copy, Debug)]
pub(super) struct Complex {
    pub re: f64,
    pub im: f64,
}

impl Complex {
    pub const ZERO: Self = Self { re: 0.0, im: 0.0 };

    pub fn magnitude(self) -> f64 {
        self.re.hypot(self.im)
    }

    fn multiply(self, other: Self) -> Self {
        Self {
            re: self.re * other.re - self.im * other.im,
            im: self.re * other.im + self.im * other.re,
        }
    }
}

/// In-place scalar radix-2 forward DFT, unnormalized, natural-order output.
pub(super) fn transform(
    values: &mut [Complex],
    check: &mut impl FnMut(CancelPoint) -> Result<(), SpectrumError>,
) -> Result<(), SpectrumError> {
    let len = values.len();
    debug_assert!(len.is_power_of_two());
    let shift = usize::BITS - len.ilog2();
    for index in 0..len {
        if index % 1_024 == 0 {
            check(CancelPoint::Permutation)?;
        }
        let reversed = index.reverse_bits() >> shift;
        if reversed > index {
            values.swap(index, reversed);
        }
    }
    let mut width = 2;
    while width <= len {
        check(CancelPoint::FftStage)?;
        let (sin, cos) = (-std::f64::consts::TAU / width as f64).sin_cos();
        let step = Complex { re: cos, im: sin };
        for block in values.chunks_exact_mut(width) {
            let mut twiddle = Complex { re: 1.0, im: 0.0 };
            let (low, high) = block.split_at_mut(width / 2);
            for (even, odd) in low.iter_mut().zip(high) {
                let upper = *even;
                let lower = odd.multiply(twiddle);
                *even = Complex {
                    re: upper.re + lower.re,
                    im: upper.im + lower.im,
                };
                *odd = Complex {
                    re: upper.re - lower.re,
                    im: upper.im - lower.im,
                };
                twiddle = twiddle.multiply(step);
            }
        }
        width *= 2;
    }
    Ok(())
}
