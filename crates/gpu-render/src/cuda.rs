//! CUDA Driver API only: embedded PTX is JIT compiled by the installed driver.
//! Every use pushes/pops the owned context on the worker thread under one mutex.
use std::{
    ffi::{CStr, CString, c_char, c_int, c_uint, c_void},
    marker::PhantomData,
    rc::Rc,
    sync::{Arc, Mutex, OnceLock},
};

type Handle = *mut c_void;
type DevicePtr = u64;
type ResultCode = c_int;
static DEVICE: OnceLock<Result<Mutex<Compute>, String>> = OnceLock::new();
static NAME: OnceLock<String> = OnceLock::new();

macro_rules! driver_api {
    ($($field:ident: $symbol:literal => $ty:ty),+ $(,)?) => {
        struct Api { _library: libloading::Library, $($field: $ty),+ }
        impl Api {
            fn load() -> Result<Arc<Self>, String> {
                // SAFETY: search only the Windows system directory, never cwd/PATH.
                let library: libloading::Library = unsafe {
                    libloading::os::windows::Library::load_with_flags("nvcuda.dll", 0x800)
                }.map_err(|e| format!("CUDA driver unavailable: {e}"))?.into();
                // SAFETY: fixed driver exports use the documented Windows API ABI.
                $(let $field = unsafe { *library.get::<$ty>(concat!($symbol, "\0").as_bytes())
                    .map_err(|e| e.to_string())? };)+
                Ok(Arc::new(Self { _library: library, $($field),+ }))
            }
        }
    }
}
driver_api! {
    init: "cuInit" => unsafe extern "system" fn(c_uint) -> ResultCode,
    device_get: "cuDeviceGet" => unsafe extern "system" fn(*mut c_int, c_int) -> ResultCode,
    device_name: "cuDeviceGetName" => unsafe extern "system" fn(*mut c_char, c_int, c_int) -> ResultCode,
    context_create: "cuCtxCreate_v2" => unsafe extern "system" fn(*mut Handle, c_uint, c_int) -> ResultCode,
    context_destroy: "cuCtxDestroy_v2" => unsafe extern "system" fn(Handle) -> ResultCode,
    context_push: "cuCtxPushCurrent_v2" => unsafe extern "system" fn(Handle) -> ResultCode,
    context_pop: "cuCtxPopCurrent_v2" => unsafe extern "system" fn(*mut Handle) -> ResultCode,
    synchronize: "cuCtxSynchronize" => unsafe extern "system" fn() -> ResultCode,
    module_load: "cuModuleLoadData" => unsafe extern "system" fn(*mut Handle, *const c_void) -> ResultCode,
    module_unload: "cuModuleUnload" => unsafe extern "system" fn(Handle) -> ResultCode,
    function_get: "cuModuleGetFunction" => unsafe extern "system" fn(*mut Handle, Handle, *const c_char) -> ResultCode,
    alloc: "cuMemAlloc_v2" => unsafe extern "system" fn(*mut DevicePtr, usize) -> ResultCode,
    free: "cuMemFree_v2" => unsafe extern "system" fn(DevicePtr) -> ResultCode,
    upload: "cuMemcpyHtoD_v2" => unsafe extern "system" fn(DevicePtr, *const c_void, usize) -> ResultCode,
    download: "cuMemcpyDtoH_v2" => unsafe extern "system" fn(*mut c_void, DevicePtr, usize) -> ResultCode,
    launch: "cuLaunchKernel" => unsafe extern "system" fn(Handle, c_uint, c_uint, c_uint, c_uint, c_uint, c_uint, c_uint, Handle, *mut *mut c_void, *mut *mut c_void) -> ResultCode,
    error_string: "cuGetErrorString" => unsafe extern "system" fn(ResultCode, *mut *const c_char) -> ResultCode,
}
impl Api {
    fn check(&self, code: ResultCode) -> Result<(), String> {
        if code == 0 {
            return Ok(());
        }
        let mut message = std::ptr::null();
        // SAFETY: output points to a driver-owned NUL-terminated static string.
        unsafe {
            (self.error_string)(code, &mut message);
        }
        let message = if message.is_null() {
            format!("error {code}")
        } else {
            unsafe { CStr::from_ptr(message) }
                .to_string_lossy()
                .into_owned()
        };
        Err(format!("CUDA: {message} ({code})"))
    }
}
struct Current {
    api: Arc<Api>,
    _thread_bound: PhantomData<Rc<()>>,
}
impl Drop for Current {
    fn drop(&mut self) {
        let mut previous = std::ptr::null_mut();
        // SAFETY: balances exactly one successful push on this same thread.
        unsafe {
            (self.api.context_pop)(&mut previous);
        }
    }
}
struct Compute {
    api: Arc<Api>,
    context: Handle,
    module: Handle,
    blur: Handle,
    transpose: Handle,
    box3_module: Handle,
    box3: Handle,
    lut_module: Handle,
    lut: Handle,
    table: DevicePtr,
    images: [DevicePtr; 2],
    capacity: usize,
    readback: Vec<u8>,
}
// SAFETY: CUDA contexts may migrate threads via push/pop. Access is exclusively
// serialized by DEVICE's mutex; neither a kernel nor a borrowed host slice escapes.
unsafe impl Send for Compute {}
impl Compute {
    fn enter(&self) -> Result<Current, String> {
        // SAFETY: owned live context, serialized caller, thread-local push/pop.
        self.api
            .check(unsafe { (self.api.context_push)(self.context) })?;
        Ok(Current {
            api: self.api.clone(),
            _thread_bound: PhantomData,
        })
    }
    fn new() -> Result<Self, String> {
        let api = Api::load()?;
        let mut device = 0;
        let mut context = std::ptr::null_mut();
        let mut name = [0i8; 256];
        // SAFETY: initialized outputs, valid device ordinal, default context flags.
        unsafe {
            api.check((api.init)(0))?;
            api.check((api.device_get)(&mut device, 0))?;
            api.check((api.device_name)(
                name.as_mut_ptr(),
                name.len() as i32,
                device,
            ))?;
            api.check((api.context_create)(&mut context, 0, device))?;
        }
        let mut popped = std::ptr::null_mut();
        let pop = api.check(unsafe { (api.context_pop)(&mut popped) });
        if let Err(error) = pop {
            unsafe {
                (api.context_destroy)(context);
            }
            return Err(error);
        }
        let mut result = Self {
            api,
            context,
            module: std::ptr::null_mut(),
            blur: std::ptr::null_mut(),
            transpose: std::ptr::null_mut(),
            box3_module: std::ptr::null_mut(),
            box3: std::ptr::null_mut(),
            lut_module: std::ptr::null_mut(),
            lut: std::ptr::null_mut(),
            table: 0,
            images: [0; 2],
            capacity: 0,
            readback: Vec::new(),
        };
        {
            let _current = result.enter()?;
            let ptx = CString::new(include_str!("blur.ptx")).map_err(|e| e.to_string())?;
            // SAFETY: NUL-terminated embedded PTX, owned context and output handles.
            unsafe {
                result.api.check((result.api.module_load)(
                    &mut result.module,
                    ptx.as_ptr().cast(),
                ))?;
                result.api.check((result.api.function_get)(
                    &mut result.blur,
                    result.module,
                    c"column_blur".as_ptr(),
                ))?;
                result.api.check((result.api.function_get)(
                    &mut result.transpose,
                    result.module,
                    c"transpose".as_ptr(),
                ))?;
            }
        }
        let name = unsafe { CStr::from_ptr(name.as_ptr()) }
            .to_string_lossy()
            .into_owned();
        let _ = NAME.set(name);
        Ok(result)
    }
    fn buffers(&mut self, bytes: usize) -> Result<(), String> {
        if bytes <= self.capacity {
            return Ok(());
        }
        self.release_images(); // Do not retain old capacity during replacement.
        for i in 0..2 {
            if let Err(error) = self
                .api
                .check(unsafe { (self.api.alloc)(&mut self.images[i], bytes) })
            {
                self.release_images();
                return Err(error);
            }
        }
        self.capacity = bytes;
        Ok(())
    }
    fn release_images(&mut self) {
        for image in &mut self.images {
            if *image != 0 {
                unsafe {
                    (self.api.free)(*image);
                }
                *image = 0;
            }
        }
        self.capacity = 0;
    }
    fn launch(
        &self,
        function: Handle,
        grid: [u32; 2],
        block: [u32; 2],
        args: &mut [*mut c_void],
    ) -> Result<(), String> {
        // SAFETY: exact PTX parameter types in local storage; default-stream
        // launches copy those values before returning. All buffers are bounded.
        self.api.check(unsafe {
            (self.api.launch)(
                function,
                grid[0],
                grid[1],
                1,
                block[0],
                block[1],
                1,
                0,
                std::ptr::null_mut(),
                args.as_mut_ptr(),
                std::ptr::null_mut(),
            )
        })
    }
    fn transpose(
        &self,
        src: DevicePtr,
        dst: DevicePtr,
        width: u32,
        height: u32,
    ) -> Result<(), String> {
        let (mut src, mut dst, mut width, mut height) = (src, dst, width, height);
        self.launch(
            self.transpose,
            [width.div_ceil(16), height.div_ceil(16)],
            [16, 16],
            &mut [
                (&mut src as *mut DevicePtr).cast(),
                (&mut dst as *mut DevicePtr).cast(),
                (&mut width as *mut u32).cast(),
                (&mut height as *mut u32).cast(),
            ],
        )
    }
    fn column(
        &self,
        src: DevicePtr,
        dst: DevicePtr,
        width: u32,
        height: u32,
        radius: u32,
    ) -> Result<(), String> {
        let (mut src, mut dst, mut width, mut height, mut radius) =
            (src, dst, width, height, radius);
        let mut inverse = 1.0f32 / (radius * 2 + 1) as f32;
        // More resident warps than one thread per entire column. Very large
        // radii keep a whole column to avoid repeated long initial windows.
        let mut tile_height = if radius <= 128 { 256 } else { height };
        self.launch(
            self.blur,
            [width.div_ceil(128), height.div_ceil(tile_height)],
            [128, 1],
            &mut [
                (&mut src as *mut DevicePtr).cast(),
                (&mut dst as *mut DevicePtr).cast(),
                (&mut width as *mut u32).cast(),
                (&mut height as *mut u32).cast(),
                (&mut radius as *mut u32).cast(),
                (&mut inverse as *mut f32).cast(),
                (&mut tile_height as *mut u32).cast(),
            ],
        )
    }
    fn apply(
        &mut self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        radii: [[u32; 2]; 5],
        colors: Option<[&[u8; 65536]; 2]>,
    ) -> Result<(), String> {
        let _current = self.enter()?;
        self.buffers(pixels.len())?;
        self.api.check(unsafe {
            (self.api.upload)(self.images[0], pixels.as_ptr().cast(), pixels.len())
        })?;
        let mut source = 0;
        if let Some(tables) = colors {
            self.lookup_device(source, pixels.len() / 4, tables[0])?;
            source = 1 - source;
        }
        for [x, y] in radii {
            if y != 0 {
                self.column(
                    self.images[source],
                    self.images[1 - source],
                    width,
                    height,
                    y,
                )?;
                source = 1 - source;
            }
            if x != 0 {
                self.transpose(self.images[source], self.images[1 - source], width, height)?;
                source = 1 - source;
                self.column(
                    self.images[source],
                    self.images[1 - source],
                    height,
                    width,
                    x,
                )?;
                source = 1 - source;
                self.transpose(self.images[source], self.images[1 - source], height, width)?;
                source = 1 - source;
            }
        }
        if let Some(tables) = colors {
            self.lookup_device(source, pixels.len() / 4, tables[1])?;
            source = 1 - source;
        }
        self.api.check(unsafe { (self.api.synchronize)() })?;
        self.commit_readback(self.images[source], pixels)?;
        Ok(())
    }
    fn apply_box3(
        &mut self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        radii: [f64; 2],
        vertical_first: bool,
    ) -> Result<(), String> {
        let _current = self.enter()?;
        if self.box3.is_null() {
            let ptx = CString::new(include_str!("box3.ptx")).map_err(|e| e.to_string())?;
            // SAFETY: embedded module and fixed parameter ABI; owned context is current.
            unsafe {
                self.api.check((self.api.module_load)(
                    &mut self.box3_module,
                    ptx.as_ptr().cast(),
                ))?;
                self.api.check((self.api.function_get)(
                    &mut self.box3,
                    self.box3_module,
                    c"fractional_column".as_ptr(),
                ))?;
            }
        }
        self.buffers(pixels.len())?;
        self.api.check(unsafe {
            (self.api.upload)(self.images[0], pixels.as_ptr().cast(), pixels.len())
        })?;
        let mut source = 0;
        for axis in if vertical_first { [1, 0] } else { [0, 1] } {
            let radius = radii[axis];
            if radius <= 0.5 {
                continue;
            }
            let (mut w, mut h) = if axis == 0 {
                (height, width)
            } else {
                (width, height)
            };
            if axis == 0 {
                self.transpose(self.images[source], self.images[1 - source], width, height)?;
                source = 1 - source;
            }
            let mut n = (radius + 0.5).floor() as u32;
            let mut weight = radius - (f64::from(n) - 0.5);
            let mut divisor = radius * 2.0;
            for _ in 0..3 {
                let (mut src, mut dst) = (self.images[source], self.images[1 - source]);
                self.launch(
                    self.box3,
                    [w.div_ceil(128), 1],
                    [128, 1],
                    &mut [
                        (&mut src as *mut DevicePtr).cast(),
                        (&mut dst as *mut DevicePtr).cast(),
                        (&mut w as *mut u32).cast(),
                        (&mut h as *mut u32).cast(),
                        (&mut n as *mut u32).cast(),
                        (&mut weight as *mut f64).cast(),
                        (&mut divisor as *mut f64).cast(),
                    ],
                )?;
                source = 1 - source;
            }
            if axis == 0 {
                self.transpose(self.images[source], self.images[1 - source], height, width)?;
                source = 1 - source;
            }
        }
        self.api.check(unsafe { (self.api.synchronize)() })?;
        self.commit_readback(self.images[source], pixels)?;
        Ok(())
    }
    fn lookup_device(
        &mut self,
        source: usize,
        pixels: usize,
        table: &[u8; 65536],
    ) -> Result<(), String> {
        if self.lut.is_null() {
            let ptx = CString::new(include_str!("lut.ptx")).map_err(|e| e.to_string())?;
            // SAFETY: fixed PTX ABI and bounded 64 KiB table, owned current context.
            unsafe {
                self.api.check((self.api.module_load)(
                    &mut self.lut_module,
                    ptx.as_ptr().cast(),
                ))?;
                self.api.check((self.api.function_get)(
                    &mut self.lut,
                    self.lut_module,
                    c"channel_lut".as_ptr(),
                ))?;
            }
        }
        self.api.check(unsafe { (self.api.synchronize)() })?;
        if self.table == 0 {
            self.api
                .check(unsafe { (self.api.alloc)(&mut self.table, table.len()) })?;
        }
        self.api
            .check(unsafe { (self.api.upload)(self.table, table.as_ptr().cast(), table.len()) })?;
        let (mut src, mut dst, mut lookup) =
            (self.images[source], self.images[1 - source], self.table);
        let mut count = pixels as u32;
        self.launch(
            self.lut,
            [count.div_ceil(256), 1],
            [256, 1],
            &mut [
                (&mut src as *mut DevicePtr).cast(),
                (&mut dst as *mut DevicePtr).cast(),
                (&mut lookup as *mut DevicePtr).cast(),
                (&mut count as *mut u32).cast(),
            ],
        )
    }
    fn commit_readback(&mut self, source: DevicePtr, pixels: &mut [u8]) -> Result<(), String> {
        if self.readback.len() < pixels.len() {
            self.readback = Vec::new();
            self.readback
                .try_reserve_exact(pixels.len())
                .map_err(|_| "CUDA readback allocation failed".to_string())?;
            self.readback.resize(pixels.len(), 0u8);
        }
        self.api.check(unsafe {
            (self.api.download)(self.readback.as_mut_ptr().cast(), source, pixels.len())
        })?;
        pixels.copy_from_slice(&self.readback[..pixels.len()]);
        Ok(())
    }
    fn apply_lut(&mut self, pixels: &mut [u8], table: &[u8; 65536]) -> Result<(), String> {
        let _current = self.enter()?;
        self.buffers(pixels.len())?;
        self.api.check(unsafe {
            (self.api.upload)(self.images[0], pixels.as_ptr().cast(), pixels.len())
        })?;
        self.lookup_device(0, pixels.len() / 4, table)?;
        self.api.check(unsafe { (self.api.synchronize)() })?;
        self.commit_readback(self.images[1], pixels)
    }
}
impl Drop for Compute {
    fn drop(&mut self) {
        if let Ok(_current) = self.enter() {
            self.release_images();
            if self.table != 0 {
                unsafe {
                    (self.api.free)(self.table);
                }
            }
            if !self.lut_module.is_null() {
                unsafe {
                    (self.api.module_unload)(self.lut_module);
                }
            }
            if !self.module.is_null() {
                unsafe {
                    (self.api.module_unload)(self.module);
                }
            }
            if !self.box3_module.is_null() {
                unsafe {
                    (self.api.module_unload)(self.box3_module);
                }
            }
        }
        unsafe {
            (self.api.context_destroy)(self.context);
        }
    }
}
pub(super) fn initialize() -> Result<(), String> {
    DEVICE
        .get_or_init(|| Compute::new().map(Mutex::new))
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}
pub(super) fn adapter_name() -> Option<String> {
    NAME.get().cloned()
}
pub(super) fn apply(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    radii: [[u32; 2]; 5],
) -> Result<(), String> {
    let mut compute = DEVICE
        .get_or_init(|| Compute::new().map(Mutex::new))
        .as_ref()
        .map_err(Clone::clone)?
        .lock()
        .map_err(|_| "CUDA context poisoned".to_string())?;
    let halo = radii
        .into_iter()
        .fold([0u32; 2], |sum, r| [sum[0] + r[0], sum[1] + r[1]]);
    if let Some(mut crop) = super::crop::Crop::new(pixels, width, height, halo) {
        compute.apply(&mut crop.pixels, crop.width, crop.height, radii, None)?;
        crop.commit(pixels, width);
        Ok(())
    } else {
        compute.apply(pixels, width, height, radii, None)
    }
}
pub(super) fn apply_box3(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    radii: [f64; 2],
    vertical_first: bool,
) -> Result<(), String> {
    let mut compute = DEVICE
        .get_or_init(|| Compute::new().map(Mutex::new))
        .as_ref()
        .map_err(Clone::clone)?
        .lock()
        .map_err(|_| "CUDA context poisoned".to_string())?;
    let halo = radii.map(|r| {
        if r <= 0.5 {
            0
        } else {
            3 * (r + 0.5).floor() as u32
        }
    });
    if let Some(mut crop) = super::crop::Crop::new(pixels, width, height, halo) {
        compute.apply_box3(
            &mut crop.pixels,
            crop.width,
            crop.height,
            radii,
            vertical_first,
        )?;
        crop.commit(pixels, width);
        Ok(())
    } else {
        compute.apply_box3(pixels, width, height, radii, vertical_first)
    }
}
pub(super) fn apply_lut(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    table: &[u8; 65536],
) -> Result<(), String> {
    let mut compute = DEVICE
        .get_or_init(|| Compute::new().map(Mutex::new))
        .as_ref()
        .map_err(Clone::clone)?
        .lock()
        .map_err(|_| "CUDA context poisoned".to_string())?;
    if table[0] == 0 {
        if let Some(mut crop) = super::crop::Crop::new(pixels, width, height, [0; 2]) {
            compute.apply_lut(&mut crop.pixels, table)?;
            crop.commit(pixels, width);
            return Ok(());
        }
    }
    compute.apply_lut(pixels, table)
}

pub(super) fn apply_color_blur(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    radii: [[u32; 2]; 5],
    tables: [&[u8; 65536]; 2],
) -> Result<(), String> {
    let mut compute = DEVICE
        .get_or_init(|| Compute::new().map(Mutex::new))
        .as_ref()
        .map_err(Clone::clone)?
        .lock()
        .map_err(|_| "CUDA context poisoned".to_string())?;
    let halo = radii
        .into_iter()
        .fold([0u32; 2], |s, r| [s[0] + r[0], s[1] + r[1]]);
    if tables[0][0] == 0 && tables[1][0] == 0 {
        if let Some(mut crop) = super::crop::Crop::new(pixels, width, height, halo) {
            compute.apply(
                &mut crop.pixels,
                crop.width,
                crop.height,
                radii,
                Some(tables),
            )?;
            crop.commit(pixels, width);
            return Ok(());
        }
    }
    compute.apply(pixels, width, height, radii, Some(tables))
}
