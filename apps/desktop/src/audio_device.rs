//! WASAPI objects and their buffers never leave this dedicated COM thread.
use windows::Win32::{Media::Audio::*, System::Com::*};

pub(super) struct Apartment;
impl Apartment {
    pub fn new() -> Result<Self, String> {
        // SAFETY: called once on a new worker, balanced after all COM objects drop.
        unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() }
            .map_err(|e| e.to_string())?;
        Ok(Self)
    }
}
impl Drop for Apartment {
    fn drop(&mut self) {
        // SAFETY: this thread successfully initialized COM in new().
        unsafe { CoUninitialize() }
    }
}

pub(super) struct Device {
    client: IAudioClient,
    render: IAudioRenderClient,
    clock: IAudioClock,
    frequency: u64,
    pub capacity: u32,
}
impl Device {
    pub fn open() -> Result<Self, String> {
        // SAFETY: all interfaces are created, used and released on this COM thread.
        // Initialize copies the stack format; GetService returns owned interfaces.
        unsafe {
            let enumerator: IMMDeviceEnumerator =
                CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
                    .map_err(|e| e.to_string())?;
            let endpoint = enumerator
                .GetDefaultAudioEndpoint(eRender, eMultimedia)
                .map_err(|e| format!("No default audio output: {e}"))?;
            let client: IAudioClient = endpoint
                .Activate(CLSCTX_ALL, None)
                .map_err(|e| e.to_string())?;
            let format = WAVEFORMATEX {
                wFormatTag: 3, // WAVE_FORMAT_IEEE_FLOAT
                nChannels: 2,
                nSamplesPerSec: 48_000,
                nAvgBytesPerSec: 384_000,
                nBlockAlign: 8,
                wBitsPerSample: 32,
                cbSize: 0,
            };
            client
                .Initialize(
                    AUDCLNT_SHAREMODE_SHARED,
                    AUDCLNT_STREAMFLAGS_AUTOCONVERTPCM | AUDCLNT_STREAMFLAGS_SRC_DEFAULT_QUALITY,
                    1_000_000,
                    0,
                    &format,
                    None,
                )
                .map_err(|e| format!("Cannot open audio output: {e}"))?;
            let render = client.GetService().map_err(|e| e.to_string())?;
            let clock: IAudioClock = client.GetService().map_err(|e| e.to_string())?;
            let frequency = clock.GetFrequency().map_err(|e| e.to_string())?;
            let capacity = client.GetBufferSize().map_err(|e| e.to_string())?;
            if frequency == 0 || capacity == 0 || capacity > 96_000 {
                return Err("Invalid audio-device buffer or clock".into());
            }
            Ok(Self {
                client,
                render,
                clock,
                frequency,
                capacity,
            })
        }
    }
    pub fn padding(&self) -> Result<u32, String> {
        // SAFETY: initialized shared-mode client on its owning thread.
        unsafe { self.client.GetCurrentPadding() }.map_err(|e| e.to_string())
    }
    pub fn position(&self) -> Result<u64, String> {
        let mut position = 0;
        // SAFETY: valid output pointer, live clock on its owning thread.
        unsafe { self.clock.GetPosition(&mut position, None) }.map_err(|e| e.to_string())?;
        Ok((u128::from(position) * 48_000 / u128::from(self.frequency)) as u64)
    }
    pub fn write(&self, pcm: &[[f32; 2]]) -> Result<(), String> {
        let count = u32::try_from(pcm.len()).map_err(|e| e.to_string())?;
        if count == 0 {
            return Ok(());
        }
        // SAFETY: GetBuffer reserves count frames in the negotiated stereo f32
        // format. Copy exactly those bytes, then release exactly once. No borrowed
        // buffer escapes, no allocation or fallible operation occurs between calls.
        unsafe {
            let buffer = self.render.GetBuffer(count).map_err(|e| e.to_string())?;
            std::ptr::copy_nonoverlapping(pcm.as_ptr().cast::<u8>(), buffer, pcm.len() * 8);
            self.render
                .ReleaseBuffer(count, 0)
                .map_err(|e| e.to_string())
        }
    }
    pub fn start(&self) -> Result<(), String> {
        // SAFETY: initialized client; the transport calls this only when stopped.
        unsafe { self.client.Start() }.map_err(|e| e.to_string())
    }
    pub fn reset(&self) -> Result<(), String> {
        // SAFETY: no outstanding render buffer; Stop precedes Reset.
        unsafe { self.client.Stop().and_then(|_| self.client.Reset()) }.map_err(|e| e.to_string())
    }
}
impl Drop for Device {
    fn drop(&mut self) {
        // SAFETY: still on the owning thread, no outstanding buffer.
        unsafe {
            let _ = self.client.Stop();
            let _ = self.client.Reset();
        }
    }
}
