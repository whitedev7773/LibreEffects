// Copyright 2026 the Libre Effects Authors
// SPDX-License-Identifier: Apache-2.0 OR MIT
use std::{cell::RefCell, marker::PhantomData, rc::Rc, sync::Arc};

/// Optional exact-byte acceleration of the existing five-pass Gaussian box kernel.
pub trait BoxBlurAccelerator: Send + Sync {
    /// Five vertical/horizontal radius pairs, in the original quantized order.
    /// Return true only after a complete result; false must leave pixels unchanged.
    fn apply(&self, pixels: &mut [u8], width: u32, height: u32, radii: [[u32; 2]; 5]) -> bool;
    /// Optional fractional box profile: three passes per axis, exact f64 math.
    /// False must leave pixels unchanged; the checked CPU fallback remains active.
    fn apply_box3(
        &self,
        _pixels: &mut [u8],
        _width: u32,
        _height: u32,
        _radii: [f64; 2],
        _vertical_first: bool,
    ) -> bool {
        false
    }
    /// Applies an exact RGB lookup indexed by alpha and channel; preserves alpha.
    fn apply_lut(
        &self,
        _pixels: &mut [u8],
        _width: u32,
        _height: u32,
        _table: &[u8; 65536],
    ) -> bool {
        false
    }
    /// Optional device-resident input LUT, five-pass Gaussian, output LUT.
    /// A declined/failed job must leave the entire source unchanged.
    fn apply_color_blur(
        &self,
        _pixels: &mut [u8],
        _width: u32,
        _height: u32,
        _radii: [[u32; 2]; 5],
        _tables: [&[u8; 65536]; 2],
    ) -> bool {
        false
    }
}
thread_local! {
    static CURRENT: RefCell<Option<Arc<dyn BoxBlurAccelerator>>> = RefCell::new(None);
}
/// Restores the preceding thread's accelerator on drop, including unwinding.
/// A scope cannot move to another thread; nested renders preserve their caller.
pub struct BoxBlurAcceleratorGuard {
    previous: Option<Arc<dyn BoxBlurAccelerator>>,
    thread: PhantomData<Rc<()>>,
}
impl Drop for BoxBlurAcceleratorGuard {
    fn drop(&mut self) {
        CURRENT.with(|slot| *slot.borrow_mut() = self.previous.take());
    }
}
/// Selects a safe external accelerator for this synchronous render scope only.
pub fn install_box_blur_accelerator(
    accelerator: Option<Arc<dyn BoxBlurAccelerator>>,
) -> BoxBlurAcceleratorGuard {
    BoxBlurAcceleratorGuard {
        previous: CURRENT.with(|slot| slot.replace(accelerator)),
        thread: PhantomData,
    }
}
pub(crate) fn apply(pixels: &mut [u8], width: u32, height: u32, radii: [[u32; 2]; 5]) -> bool {
    let accelerator = CURRENT.with(|slot| slot.borrow().clone());
    accelerator.is_some_and(|accelerator| accelerator.apply(pixels, width, height, radii))
}
pub(crate) fn apply_box3(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    radii: [f64; 2],
    vertical_first: bool,
) -> bool {
    let accelerator = CURRENT.with(|slot| slot.borrow().clone());
    accelerator.is_some_and(|a| a.apply_box3(pixels, width, height, radii, vertical_first))
}
pub(crate) fn apply_lut(pixels: &mut [u8], width: u32, height: u32, table: &[u8; 65536]) -> bool {
    let accelerator = CURRENT.with(|slot| slot.borrow().clone());
    accelerator.is_some_and(|a| a.apply_lut(pixels, width, height, table))
}
pub(crate) fn apply_color_blur(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    radii: [[u32; 2]; 5],
    tables: [&[u8; 65536]; 2],
) -> bool {
    let accelerator = CURRENT.with(|slot| slot.borrow().clone());
    accelerator.is_some_and(|a| a.apply_color_blur(pixels, width, height, radii, tables))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    struct Decline(AtomicUsize);
    impl BoxBlurAccelerator for Decline {
        fn apply(&self, _: &mut [u8], _: u32, _: u32, _: [[u32; 2]; 5]) -> bool {
            self.0.fetch_add(1, Ordering::Relaxed);
            false
        }
    }
    #[test]
    fn scoped_backend_restores_nested_and_unwound_calls_without_thread_leaks() {
        let backend = Arc::new(Decline(AtomicUsize::new(0)));
        let mut bytes = [9; 4];
        {
            let _parent = install_box_blur_accelerator(Some(backend.clone()));
            assert!(!apply(&mut bytes, 1, 1, [[1; 2]; 5]));
            {
                let _child = install_box_blur_accelerator(None);
                assert!(!apply(&mut bytes, 1, 1, [[1; 2]; 5]));
            }
            let _ = std::panic::catch_unwind(|| {
                let _child = install_box_blur_accelerator(None);
                panic!("restore");
            });
            assert!(!apply(&mut bytes, 1, 1, [[1; 2]; 5]));
            std::thread::spawn(|| assert!(!apply(&mut [9; 4], 1, 1, [[1; 2]; 5])))
                .join()
                .unwrap();
        }
        assert!(!apply(&mut bytes, 1, 1, [[1; 2]; 5]));
        assert_eq!(backend.0.load(Ordering::Relaxed), 2);
        assert_eq!(bytes, [9; 4]);
    }
    #[test]
    fn declined_hardware_leaves_complete_cpu_render_identical() {
        let source = r##"<svg xmlns="http://www.w3.org/2000/svg" width="32" height="24"><defs><filter id="b"><feGaussianBlur stdDeviation="4 6"/></filter></defs><rect x="5" y="4" width="12" height="9" fill="#204060" filter="url(#b)"/></svg>"##;
        let tree = usvg::Tree::from_str(source, &usvg::Options::default()).unwrap();
        let mut expected = tiny_skia::Pixmap::new(32, 24).unwrap();
        crate::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut expected.as_mut(),
        );
        let backend = Arc::new(Decline(AtomicUsize::new(0)));
        let _scope = install_box_blur_accelerator(Some(backend.clone()));
        let mut actual = tiny_skia::Pixmap::new(32, 24).unwrap();
        crate::render(
            &tree,
            tiny_skia::Transform::identity(),
            &mut actual.as_mut(),
        );
        assert_eq!(actual.data(), expected.data());
        assert!(backend.0.load(Ordering::Relaxed) > 0);
    }
}
