//! Letting MeshPeek draw the thumbnails of file types another program draws so far, for the current user. The
//! installer registers the provider itself and fills the gaps; this goes one step further and can be undone: what was
//! registered before is kept under `HKCU\Software\MeshPeek\Thumbnails`.

use std::ffi::c_void;

use meshpeek_core::SUPPORTED_EXTENSIONS;
use windows::Win32::System::Registry::{
    HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ, RegDeleteKeyValueW, RegDeleteTreeW,
    RegGetValueW, RegSetKeyValueW,
};
use windows::Win32::UI::Shell::{SHCNE_ASSOCCHANGED, SHCNF_IDLIST, SHChangeNotify};
use windows::core::{HSTRING, Result};

/// The provider's class id, as the installer registers it.
pub const CLASS: &str = "{8C5B6D0E-3F0A-4A3D-9B52-71D7A4C2E6F1}";
/// The key under a file type that names its thumbnail provider.
const HANDLER: &str = "shellex\\{e357fccd-a995-4576-b01f-234630154e96}";
/// The providers replaced, by extension (empty: the user had none).
const REPLACED: &str = "Software\\MeshPeek\\Thumbnails";

/// The provider is installed.
pub fn installed() -> bool {
    get(HKEY_CLASSES_ROOT, &format!("CLSID\\{CLASS}\\InprocServer32"), "").is_some()
}

/// MeshPeek took over the other programs' thumbnails.
pub fn replacing() -> bool {
    SUPPORTED_EXTENSIONS.iter().any(|e| get(HKEY_CURRENT_USER, REPLACED, e).is_some())
}

/// Take over the thumbnails of all supported file types.
pub fn replace() -> Result<()> {
    replace_in("Software\\Classes", REPLACED)?;
    notify();
    Ok(())
}

/// Give them back to the programs that drew them before (types another program took since are left alone).
pub fn restore() -> Result<()> {
    restore_in("Software\\Classes", REPLACED)?;
    notify();
    Ok(())
}

/// Tell Explorer that the registration changed.
pub fn notify() {
    unsafe { SHChangeNotify(SHCNE_ASSOCCHANGED, SHCNF_IDLIST, None, None) };
}

fn replace_in(classes: &str, replaced: &str) -> Result<()> {
    for extension in SUPPORTED_EXTENSIONS {
        let key = format!("{classes}\\.{extension}\\{HANDLER}");
        let before = get(HKEY_CURRENT_USER, &key, "").unwrap_or_default();
        if !before.eq_ignore_ascii_case(CLASS) {
            set(replaced, extension, &before)?; // first, so that a failure afterwards loses nothing
            set(&key, "", CLASS)?;
        }
    }
    Ok(())
}

fn restore_in(classes: &str, replaced: &str) -> Result<()> {
    for extension in SUPPORTED_EXTENSIONS {
        let Some(before) = get(HKEY_CURRENT_USER, replaced, extension) else { continue };
        let key = format!("{classes}\\.{extension}\\{HANDLER}");
        if get(HKEY_CURRENT_USER, &key, "").is_some_and(|now| now.eq_ignore_ascii_case(CLASS)) {
            if before.is_empty() {
                unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(&key)).ok()? };
            } else {
                set(&key, "", &before)?;
            }
        }
        unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, &HSTRING::from(replaced), &HSTRING::from(*extension)).ok()? };
    }
    Ok(())
}

/// A text value (`""`: the key's default value).
fn get(root: HKEY, key: &str, name: &str) -> Option<String> {
    let mut text = [0u16; 512];
    let mut bytes = size_of_val(&text) as u32;
    let (key, name) = (HSTRING::from(key), HSTRING::from(name));
    let into = Some(text.as_mut_ptr() as *mut c_void);
    unsafe { RegGetValueW(root, &key, &name, RRF_RT_REG_SZ, None, into, Some(&mut bytes)).ok().ok()? };
    Some(String::from_utf16_lossy(&text[..(bytes as usize / 2).saturating_sub(1)]))
}

/// Set a text value of the current user, making the key.
fn set(key: &str, name: &str, value: &str) -> Result<()> {
    let text: Vec<u16> = value.encode_utf16().chain([0]).collect();
    let (key, name) = (HSTRING::from(key), HSTRING::from(name));
    let data = Some(text.as_ptr() as *const c_void);
    unsafe { RegSetKeyValueW(HKEY_CURRENT_USER, &key, &name, REG_SZ.0, data, size_of_val(&text[..]) as u32).ok() }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replaced_handlers_come_back() {
        let root = format!("Software\\MeshPeekTest{}", std::process::id());
        let (classes, replaced) = (format!("{root}\\Classes"), format!("{root}\\Replaced"));
        let handler = |e: &str| get(HKEY_CURRENT_USER, &format!("{classes}\\.{e}\\{HANDLER}"), "");
        set(&format!("{classes}\\.ply\\{HANDLER}"), "", "{other}").unwrap();
        replace_in(&classes, &replaced).unwrap();
        replace_in(&classes, &replaced).unwrap(); // again: must not take itself for the one before
        assert!(SUPPORTED_EXTENSIONS.iter().all(|e| handler(e).as_deref() == Some(CLASS)));
        set(&format!("{classes}\\.obj\\{HANDLER}"), "", "{newer}").unwrap(); // another program came later
        restore_in(&classes, &replaced).unwrap();
        assert_eq!(handler("ply").as_deref(), Some("{other}"));
        assert_eq!(handler("stl"), None);
        assert_eq!(handler("obj").as_deref(), Some("{newer}"));
        assert_eq!(get(HKEY_CURRENT_USER, &replaced, "ply"), None);
        unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, &HSTRING::from(&root)).ok().unwrap() };
    }
}
