//! Exact transparent-support reduction; no viewport or quality approximations.
pub(super) struct Crop {
    pub x: usize,
    pub y: usize,
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}
impl Crop {
    pub fn new(input: &[u8], width: u32, height: u32, halo: [u32; 2]) -> Option<Self> {
        if input.len() < 1024 * 1024 {
            return None;
        }
        let stride = width as usize * 4;
        let visible = |row: &[u8]| row.chunks_exact(4).any(|p| p != [0; 4]);
        let first = input.chunks_exact(stride).position(visible)?;
        let last = input.chunks_exact(stride).rposition(visible)?;
        let (mut left, mut right) = (width as usize, 0usize);
        for row in input
            .chunks_exact(stride)
            .skip(first)
            .take(last - first + 1)
        {
            if let Some(x) = row.chunks_exact(4).position(|p| p != [0; 4]) {
                left = left.min(x);
                right = right.max(row.chunks_exact(4).rposition(|p| p != [0; 4]).unwrap() + 1);
                if left == 0 && right == width as usize {
                    break;
                }
            }
        }
        let x = left.saturating_sub(halo[0] as usize);
        let y = first.saturating_sub(halo[1] as usize);
        let end_x = (right + halo[0] as usize).min(width as usize);
        let end_y = (last + 1 + halo[1] as usize).min(height as usize);
        let (w, h) = (end_x - x, end_y - y);
        // Copying is worthwhile only when at least a quarter of work disappears.
        if w * h * 4 >= input.len() * 3 / 4 {
            return None;
        }
        let mut pixels = Vec::new();
        pixels.try_reserve_exact(w * h * 4).ok()?;
        for row in input.chunks_exact(stride).skip(y).take(h) {
            pixels.extend_from_slice(&row[x * 4..end_x * 4]);
        }
        Some(Self {
            x,
            y,
            width: w as u32,
            height: h as u32,
            pixels,
        })
    }
    pub fn commit(&self, output: &mut [u8], width: u32) {
        let stride = width as usize * 4;
        for (row, source) in output
            .chunks_exact_mut(stride)
            .skip(self.y)
            .zip(self.pixels.chunks_exact(self.width as usize * 4))
        {
            row[self.x * 4..(self.x + self.width as usize) * 4].copy_from_slice(source);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn support_includes_rgb_with_zero_alpha_and_every_finite_pass_halo() {
        let mut input = vec![0; 1024 * 512 * 4];
        input[(40 * 1024 + 31) * 4] = 17;
        input[(43 * 1024 + 33) * 4 + 3] = 99;
        let mut crop = Crop::new(&input, 1024, 512, [15, 10]).unwrap();
        assert_eq!((crop.x, crop.y, crop.width, crop.height), (16, 30, 33, 24));
        let before = input.clone();
        crop.commit(&mut input, 1024);
        assert_eq!(input, before);
        crop.pixels.fill(0);
        crop.commit(&mut input, 1024);
        assert!(input.iter().all(|v| *v == 0));
        assert!(Crop::new(&vec![255; input.len()], 1024, 512, [0; 2]).is_none());
        assert!(Crop::new(&before, 1024, 512, [8192; 2]).is_none());
    }
}
