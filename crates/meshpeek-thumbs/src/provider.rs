//! The COM side: class factory and thumbnail provider.

use std::cell::RefCell;
use std::ffi::c_void;
use std::path::PathBuf;
use std::sync::{Mutex, Once};

use windows::Win32::Foundation::{BOOL, CLASS_E_CLASSNOTAVAILABLE, CLASS_E_NOAGGREGATION, E_FAIL, S_FALSE};
use windows::Win32::Graphics::Gdi::{BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateDIBSection, DIB_RGB_COLORS, HBITMAP};
use windows::Win32::System::Com::{
    CoTaskMemFree, IClassFactory, IClassFactory_Impl, IStream, STATFLAG_DEFAULT, STATSTG,
};
use windows::Win32::UI::Shell::PropertiesSystem::{IInitializeWithStream, IInitializeWithStream_Impl};
use windows::Win32::UI::Shell::{IThumbnailProvider, IThumbnailProvider_Impl, WTS_ALPHATYPE, WTSAT_ARGB};
use windows::core::{GUID, HRESULT, IUnknown, Interface, Result, implement};

use crate::{MAX_BYTES, Picture, stored, thumbnail};

/// The provider's class id (`register::CLASS`).
const CLSID: GUID = GUID::from_u128(0x8c5b6d0e_3f0a_4a3d_9b52_71d7a4c2e6f1);
/// Bytes read first: a preview picture starts a file, so the rest of it may not be needed.
const PREFIX: usize = 32 << 20;
/// Stack of the loading threads, as in the app: the parsers need more than the default.
const STACK: usize = 64 << 20;

#[implement(IThumbnailProvider, IInitializeWithStream)]
#[derive(Default)]
struct Provider {
    stream: RefCell<Option<IStream>>,
}

impl IInitializeWithStream_Impl for Provider_Impl {
    fn Initialize(&self, stream: Option<&IStream>, _mode: u32) -> Result<()> {
        self.stream.replace(Some(stream.ok_or(E_FAIL)?.clone()));
        Ok(())
    }
}

impl IThumbnailProvider_Impl for Provider_Impl {
    fn GetThumbnail(&self, size: u32, bitmap: *mut HBITMAP, alpha: *mut WTS_ALPHATYPE) -> Result<()> {
        let stream = self.stream.borrow_mut().take().ok_or(E_FAIL)?;
        let (name, length) = describe(&stream)?;
        let mut data = Vec::new();
        read(&stream, &mut data, length.min(PREFIX as u64) as usize)?;
        let (found, name, mut data) = on_big_stack(move || (stored(&name, &data, size), name, data))?;
        let picture = match found {
            Some(picture) => picture,
            None if length > MAX_BYTES => return Err(E_FAIL.into()),
            None => {
                read(&stream, &mut data, length as usize)?;
                on_big_stack(move || thumbnail(&name, data, size))?.ok_or(E_FAIL)?
            }
        };
        unsafe {
            *bitmap = to_bitmap(&picture)?;
            *alpha = WTSAT_ARGB;
        }
        Ok(())
    }
}

/// The file's name and length.
fn describe(stream: &IStream) -> Result<(PathBuf, u64)> {
    let mut stat = STATSTG::default();
    unsafe {
        stream.Stat(&mut stat, STATFLAG_DEFAULT)?;
        if stat.pwcsName.is_null() {
            return Err(E_FAIL.into());
        }
        let name = stat.pwcsName.to_string();
        CoTaskMemFree(Some(stat.pwcsName.0 as *const c_void));
        Ok((PathBuf::from(name.map_err(|_| E_FAIL)?), stat.cbSize))
    }
}

/// Read on until `data` holds `length` bytes or the stream ends.
fn read(stream: &IStream, data: &mut Vec<u8>, length: usize) -> Result<()> {
    let mut at = data.len();
    data.resize(length.max(at), 0);
    while at < data.len() {
        let want = (data.len() - at).min(4 << 20) as u32;
        let mut got = 0;
        unsafe { stream.Read(data[at..].as_mut_ptr() as *mut c_void, want, Some(&mut got)).ok()? };
        if got == 0 {
            break;
        }
        at += got.min(want) as usize;
    }
    data.truncate(at);
    Ok(())
}

/// Run `work` on a thread with a big stack, one at a time; a panic in it (a file the parsers trip over) becomes an
/// error. `work` must not call COM, and no lock may be held around COM calls: while one waits, Windows lets this
/// thread serve the next request, which would then wait for the lock for ever.
fn on_big_stack<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    static POOL: Once = Once::new();
    POOL.call_once(|| {
        // few threads: Explorer should stay usable while a folder's thumbnails are made
        let threads = std::thread::available_parallelism().map_or(1, |n| n.get().div_ceil(2).min(4));
        let _ = rayon::ThreadPoolBuilder::new().num_threads(threads).stack_size(STACK).build_global();
    });
    let one_at_a_time = move || {
        static ONE: Mutex<()> = Mutex::new(()); // each file can need a lot of memory
        let _one = ONE.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        work()
    };
    let thread = std::thread::Builder::new().stack_size(STACK).spawn(one_at_a_time).map_err(|_| E_FAIL)?;
    thread.join().map_err(|_| E_FAIL.into())
}

/// The picture as the bitmap Explorer takes over.
fn to_bitmap(picture: &Picture) -> Result<HBITMAP> {
    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: picture.width as i32,
            biHeight: -(picture.height as i32), // top row first
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    unsafe {
        let mut bits = std::ptr::null_mut();
        let bitmap = CreateDIBSection(None, &info, DIB_RGB_COLORS, &mut bits, None, 0)?;
        let pixels = std::slice::from_raw_parts_mut(bits as *mut [u8; 4], picture.pixels.len());
        for (to, [r, g, b, a]) in pixels.iter_mut().zip(&picture.pixels) {
            *to = [*b, *g, *r, *a];
        }
        Ok(bitmap)
    }
}

#[implement(IClassFactory)]
struct Factory;

impl IClassFactory_Impl for Factory_Impl {
    fn CreateInstance(&self, outer: Option<&IUnknown>, iid: *const GUID, object: *mut *mut c_void) -> Result<()> {
        if outer.is_some() {
            return Err(CLASS_E_NOAGGREGATION.into());
        }
        let provider: IUnknown = Provider::default().into();
        unsafe { provider.query(iid, object).ok() }
    }

    fn LockServer(&self, _lock: BOOL) -> Result<()> {
        Ok(())
    }
}

/// # Safety
/// Called by COM with valid pointers.
#[unsafe(no_mangle)]
unsafe extern "system" fn DllGetClassObject(class: *const GUID, iid: *const GUID, object: *mut *mut c_void) -> HRESULT {
    unsafe {
        if *class != CLSID {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        let factory: IClassFactory = Factory.into();
        factory.query(iid, object)
    }
}

/// Never unloaded while its process lives: worker threads may still run its code.
#[unsafe(no_mangle)]
extern "system" fn DllCanUnloadNow() -> HRESULT {
    S_FALSE
}
