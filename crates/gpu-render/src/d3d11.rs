//! FFI is confined to owned COM resources and validated byte slices. One mutex
//! serializes the immediate context; a failed readback never reaches the caller.
use std::{
    mem::size_of,
    sync::{Mutex, OnceLock},
};
use windows::{
    Win32::{
        Foundation::HMODULE,
        Graphics::{
            Direct3D::Fxc::{
                D3DCOMPILE_ENABLE_STRICTNESS, D3DCOMPILE_IEEE_STRICTNESS,
                D3DCOMPILE_OPTIMIZATION_LEVEL3, D3DCompile,
            },
            Direct3D::{D3D_DRIVER_TYPE_UNKNOWN, D3D_FEATURE_LEVEL_11_0, ID3DInclude},
            Direct3D11::*,
            Dxgi::*,
        },
    },
    core::s,
};

static DEVICE: OnceLock<Result<Mutex<Compute>, String>> = OnceLock::new();
static ADAPTER: OnceLock<String> = OnceLock::new();
fn missing_resource() -> windows::core::Error {
    windows::core::Error::new(
        windows::core::HRESULT(0x80004005u32 as i32),
        "GPU resource is missing",
    )
}
pub(super) fn adapter_name() -> Option<String> {
    ADAPTER.get().cloned()
}
pub(super) fn initialize() -> Result<(), String> {
    DEVICE
        .get_or_init(|| Compute::new().map(Mutex::new))
        .as_ref()
        .map(|_| ())
        .map_err(Clone::clone)
}
pub(super) fn apply(
    pixels: &mut [u8],
    width: u32,
    height: u32,
    radii: [[u32; 2]; 5],
) -> Result<(), String> {
    let device = DEVICE.get_or_init(|| Compute::new().map(Mutex::new));
    let mut device = device
        .as_ref()
        .map_err(Clone::clone)?
        .lock()
        .map_err(|_| "GPU context poisoned".to_string())?;
    // Caller validation limits this to128 MiB, so ByteWidth is always a u32.
    device
        .blur(pixels, width, height, radii)
        .map_err(|e| e.to_string())
}
struct Buffer {
    resource: ID3D11Buffer,
    srv: ID3D11ShaderResourceView,
    uav: ID3D11UnorderedAccessView,
}
struct Buffers {
    bytes: usize,
    images: [Buffer; 2],
    staging: ID3D11Buffer,
}
struct Compute {
    name: String,
    device: ID3D11Device,
    context: ID3D11DeviceContext,
    shader: ID3D11ComputeShader,
    transpose_shader: ID3D11ComputeShader,
    params: ID3D11Buffer,
    buffers: Option<Buffers>,
}
#[repr(C)]
struct Params {
    width: u32,
    height: u32,
    axis: u32,
    radius: u32,
    inverse: f32,
    padding: [u32; 3],
}
impl Compute {
    fn new() -> Result<Self, String> {
        // Enumerate hardware by high-performance preference. Never use WARP,
        // reference or virtual/software adapters and claim hardware acceleration.
        let factory: IDXGIFactory6 =
            unsafe { CreateDXGIFactory2(DXGI_CREATE_FACTORY_FLAGS::default()) }
                .map_err(|e| e.to_string())?;
        let mut last_error = "No Direct3D11 hardware compute adapter".to_string();
        for index in 0..16 {
            let Ok(adapter) = (unsafe {
                factory.EnumAdapterByGpuPreference::<IDXGIAdapter1>(
                    index,
                    DXGI_GPU_PREFERENCE_HIGH_PERFORMANCE,
                )
            }) else {
                break;
            };
            let Ok(desc) = (unsafe { adapter.GetDesc1() }) else {
                continue;
            };
            if desc.Flags & DXGI_ADAPTER_FLAG_SOFTWARE.0 as u32 != 0 {
                continue;
            }
            let mut device = None;
            let mut context = None;
            let result = unsafe {
                D3D11CreateDevice(
                    &adapter,
                    D3D_DRIVER_TYPE_UNKNOWN,
                    HMODULE::default(),
                    D3D11_CREATE_DEVICE_FLAG::default(),
                    Some(&[D3D_FEATURE_LEVEL_11_0]),
                    D3D11_SDK_VERSION,
                    Some(&mut device),
                    None,
                    Some(&mut context),
                )
            };
            if let Err(error) = result {
                last_error = error.to_string();
                continue;
            }
            let (Some(device), Some(context)) = (device, context) else {
                continue;
            };
            let name = String::from_utf16_lossy(&desc.Description)
                .trim_end_matches('\0')
                .to_string();
            let compute = Self::resources(name, device, context).map_err(|e| e.to_string())?;
            let _ = ADAPTER.set(compute.name.clone());
            return Ok(compute);
        }
        Err(last_error)
    }
    fn resources(
        name: String,
        device: ID3D11Device,
        context: ID3D11DeviceContext,
    ) -> windows::core::Result<Self> {
        let shader = Self::compile_shader(&device, s!("main"))?;
        let transpose_shader = Self::compile_shader(&device, s!("transpose"))?;
        let mut params = None;
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: size_of::<Params>() as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_CONSTANT_BUFFER.0 as u32,
            ..Default::default()
        };
        unsafe {
            device.CreateBuffer(&desc, None, Some(&mut params))?;
        }
        Ok(Self {
            name,
            device,
            context,
            shader,
            transpose_shader,
            params: params.ok_or_else(missing_resource)?,
            buffers: None,
        })
    }
    fn compile_shader(
        device: &ID3D11Device,
        entry: windows::core::PCSTR,
    ) -> windows::core::Result<ID3D11ComputeShader> {
        let source = include_str!("box_blur.hlsl");
        let mut code = None;
        let mut errors = None;
        unsafe {
            D3DCompile(
                source.as_ptr().cast(),
                source.len(),
                s!("LibreEffectsBoxBlur"),
                None,
                None::<&ID3DInclude>,
                entry,
                s!("cs_5_0"),
                D3DCOMPILE_ENABLE_STRICTNESS
                    | D3DCOMPILE_IEEE_STRICTNESS
                    | D3DCOMPILE_OPTIMIZATION_LEVEL3,
                0,
                &mut code,
                Some(&mut errors),
            )?;
        }
        let code = code.ok_or_else(missing_resource)?;
        // Blob lifetime covers shader creation; D3D copies the bytecode.
        let bytes = unsafe {
            std::slice::from_raw_parts(code.GetBufferPointer().cast::<u8>(), code.GetBufferSize())
        };
        let mut shader = None;
        unsafe {
            device.CreateComputeShader(bytes, None::<&ID3D11ClassLinkage>, Some(&mut shader))?;
        }
        shader.ok_or_else(missing_resource)
    }
    fn buffers(&self, bytes: usize) -> windows::core::Result<Buffers> {
        let desc = D3D11_BUFFER_DESC {
            ByteWidth: bytes as u32,
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: (D3D11_BIND_SHADER_RESOURCE | D3D11_BIND_UNORDERED_ACCESS).0 as u32,
            MiscFlags: D3D11_RESOURCE_MISC_BUFFER_STRUCTURED.0 as u32,
            StructureByteStride: 4,
            ..Default::default()
        };
        let image = || -> windows::core::Result<Buffer> {
            let mut resource = None;
            unsafe {
                self.device.CreateBuffer(&desc, None, Some(&mut resource))?;
            }
            let resource = resource.ok_or_else(missing_resource)?;
            let mut srv = None;
            let mut uav = None;
            unsafe {
                self.device
                    .CreateShaderResourceView(&resource, None, Some(&mut srv))?;
                self.device
                    .CreateUnorderedAccessView(&resource, None, Some(&mut uav))?;
            }
            Ok(Buffer {
                resource,
                srv: srv.ok_or_else(missing_resource)?,
                uav: uav.ok_or_else(missing_resource)?,
            })
        };
        let images = [image()?, image()?];
        let staging_desc = D3D11_BUFFER_DESC {
            ByteWidth: bytes as u32,
            Usage: D3D11_USAGE_STAGING,
            CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
            ..Default::default()
        };
        let mut staging = None;
        unsafe {
            self.device
                .CreateBuffer(&staging_desc, None, Some(&mut staging))?;
        }
        Ok(Buffers {
            bytes,
            images,
            staging: staging.ok_or_else(missing_resource)?,
        })
    }
    fn blur(
        &mut self,
        pixels: &mut [u8],
        width: u32,
        height: u32,
        radii: [[u32; 2]; 5],
    ) -> windows::core::Result<()> {
        if self
            .buffers
            .as_ref()
            .is_none_or(|b| b.bytes != pixels.len())
        {
            // Release the previous capacity before allocating the next. At most
            // two128-MiB GPU images and one128-MiB readback buffer are retained.
            self.buffers = None;
            self.buffers = Some(self.buffers(pixels.len())?);
        }
        let buffers = self.buffers.as_ref().unwrap();
        let mut front = 0;
        unsafe {
            self.context.UpdateSubresource(
                &buffers.images[front].resource,
                0,
                None,
                pixels.as_ptr().cast(),
                0,
                0,
            );
            self.context
                .CSSetConstantBuffers(0, Some(&[Some(self.params.clone())]));
            for pair in radii {
                // Exactly the upstream V,H,V,H,... pass order. Radius0 is copy.
                for axis in [1, 0] {
                    let radius = pair[axis];
                    if radius == 0 {
                        continue;
                    }
                    if axis == 0 {
                        self.dispatch_transpose(buffers, &mut front, width, height);
                    }
                    let (pass_width, pass_height) = if axis == 0 {
                        (height, width)
                    } else {
                        (width, height)
                    };
                    let params = Params {
                        width: pass_width,
                        height: pass_height,
                        axis: 1,
                        radius,
                        inverse: 1.0 / (radius * 2 + 1) as f32,
                        padding: [0; 3],
                    };
                    self.dispatch(
                        buffers,
                        &mut front,
                        &self.shader,
                        params,
                        [pass_width.div_ceil(64), 1, 1],
                    );
                    if axis == 0 {
                        self.dispatch_transpose(buffers, &mut front, height, width);
                    }
                }
            }
            self.context.CSSetConstantBuffers(0, Some(&[None]));
            self.context.CSSetShader(None::<&ID3D11ComputeShader>, None);
            self.context
                .CopyResource(&buffers.staging, &buffers.images[front].resource);
            let mut mapped = D3D11_MAPPED_SUBRESOURCE::default();
            self.context
                .Map(&buffers.staging, 0, D3D11_MAP_READ, 0, Some(&mut mapped))?;
            let valid = self.device.GetDeviceRemovedReason();
            if valid.is_ok() && !mapped.pData.is_null() {
                // Both resources are owned, ByteWidth equals the verified input
                // length, and Map completed all GPU writes before this copy.
                std::ptr::copy_nonoverlapping(
                    mapped.pData.cast::<u8>(),
                    pixels.as_mut_ptr(),
                    pixels.len(),
                );
            }
            self.context.Unmap(&buffers.staging, 0);
            valid?;
            if mapped.pData.is_null() {
                return Err(missing_resource());
            }
        }
        Ok(())
    }
    fn dispatch_transpose(&self, buffers: &Buffers, front: &mut usize, width: u32, height: u32) {
        self.dispatch(
            buffers,
            front,
            &self.transpose_shader,
            Params {
                width,
                height,
                axis: 0,
                radius: 0,
                inverse: 0.0,
                padding: [0; 3],
            },
            [width.div_ceil(16), height.div_ceil(16), 1],
        );
    }
    fn dispatch(
        &self,
        buffers: &Buffers,
        front: &mut usize,
        shader: &ID3D11ComputeShader,
        params: Params,
        groups: [u32; 3],
    ) {
        unsafe {
            self.context.UpdateSubresource(
                &self.params,
                0,
                None,
                (&params as *const Params).cast(),
                0,
                0,
            );
            self.context.CSSetShader(shader, None);
            self.context
                .CSSetShaderResources(0, Some(&[Some(buffers.images[*front].srv.clone())]));
            self.context.CSSetUnorderedAccessViews(
                0,
                1,
                Some([Some(buffers.images[1 - *front].uav.clone())].as_ptr()),
                None,
            );
            self.context.Dispatch(groups[0], groups[1], groups[2]);
            self.context.CSSetShaderResources(0, Some(&[None]));
            self.context
                .CSSetUnorderedAccessViews(0, 1, Some([None].as_ptr()), None);
        }
        *front = 1 - *front;
    }
}
