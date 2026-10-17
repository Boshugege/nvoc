//! Vulkan loader discovery.
//!
//! [`ash::Entry::load`] resolves the platform's standard loader name
//! (`libvulkan.dylib` on macOS, `libvulkan.so.1`/`libvulkan.so` elsewhere).
//! That covers every mainstream install, but not the macOS NVIDIA driver stack
//! this repository targets: there the Khronos loader and the NVK ICD manifest
//! ship inside the GPU driver bundle (`/Library/GPUBundles/nvmtl/`), which is
//! neither on the dynamic-linker search path nor one of the loader's default
//! ICD directories. Without help, `--vulkan-only` fails with a bare
//! `dlopen(libvulkan.dylib)` error (no loader) or `ERROR_INCOMPATIBLE_DRIVER`
//! (loader found, no ICD) — neither of which tells the user what to install.
//!
//! [`prepare`] resolves that layout once, on the calling thread, and only
//! records what [`load_entry`] needs to open. It must run before any Vulkan
//! object exists and before any worker thread is spawned, because selecting an
//! ICD means writing `VK_DRIVER_FILES`. [`load_entry`] still tries the
//! platform lookup first and only falls back to the bundle when that fails, so
//! a stock loader install (or one already exposed through `DYLD_LIBRARY_PATH`)
//! is used exactly as before.
//!
//! Both entry points are no-ops off macOS: the candidate lists are empty and
//! the environment overrides are ignored there.

use std::path::PathBuf;
use std::sync::OnceLock;

/// Loader locations to try once the platform lookup has failed. Empty off
/// macOS so the fallback can never trigger on another platform.
#[cfg(target_os = "macos")]
const BUNDLED_LOADER_CANDIDATES: &[&str] = &[
    "/Library/GPUBundles/nvmtl/libvulkan.1.dylib",
    "/Library/GPUBundles/nvmtl/libvulkan.dylib",
];
#[cfg(not(target_os = "macos"))]
const BUNDLED_LOADER_CANDIDATES: &[&str] = &[];

/// ICD manifests to select when the caller did not pick one. Empty off macOS.
#[cfg(target_os = "macos")]
const BUNDLED_ICD_CANDIDATES: &[&str] = &["/Library/GPUBundles/nvmtl/nvk_icd.json"];
#[cfg(not(target_os = "macos"))]
const BUNDLED_ICD_CANDIDATES: &[&str] = &[];

/// Escape hatches for installs that do not use the default bundle path. Only
/// consulted on macOS.
const LOADER_PATH_ENV: &str = "NVOC_VULKAN_LOADER";
const ICD_PATH_ENV: &str = "NVOC_VULKAN_ICD";

/// The loader path resolved by [`prepare`], if the platform lookup needed a
/// macOS fallback. A plain `OnceLock` because `prepare` is expected to run on
/// the main thread; [`load_entry`] re-resolves if it did not run at all.
static RESOLVED_LOADER: OnceLock<Option<PathBuf>> = OnceLock::new();

/// Read an override path from the environment. Ignored off macOS.
fn env_path(name: &str) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    {
        let raw = std::env::var_os(name)?;
        if raw.is_empty() {
            return None;
        }
        Some(PathBuf::from(raw))
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = name;
        None
    }
}

fn first_existing(candidates: &[&str]) -> Option<PathBuf> {
    candidates
        .iter()
        .map(PathBuf::from)
        .find(|path| path.exists())
}

fn resolve_bundled_loader() -> Option<PathBuf> {
    env_path(LOADER_PATH_ENV).or_else(|| first_existing(BUNDLED_LOADER_CANDIDATES))
}

/// Select an ICD for the loader, unless the caller already chose one.
///
/// The loader reads `VK_DRIVER_FILES` when it builds the instance, so this
/// only has to happen before `create_instance`; doing it here keeps the write
/// on the main thread.
fn select_icd() {
    if BUNDLED_ICD_CANDIDATES.is_empty() {
        return;
    }
    if std::env::var_os("VK_DRIVER_FILES").is_some()
        || std::env::var_os("VK_ICD_FILENAMES").is_some()
    {
        return;
    }
    let Some(icd) = env_path(ICD_PATH_ENV).or_else(|| first_existing(BUNDLED_ICD_CANDIDATES))
    else {
        return;
    };
    // SAFETY: `prepare` runs on the main thread at the top of the stressor
    // entry point — before the render thread is spawned, before CUDA init, and
    // before any Vulkan object exists — so there is no other thread that could
    // be reading the environment concurrently, and the only reader of these
    // names (the Vulkan loader) is not loaded yet. The variable is written
    // only when the caller has not set an ICD, so an explicit choice always
    // wins over the bundle default.
    unsafe { std::env::set_var("VK_DRIVER_FILES", &icd) };
}

/// Resolve the macOS loader/ICD layout. Call once, early, on the main thread.
pub fn prepare() {
    let _ = RESOLVED_LOADER.get_or_init(resolve_bundled_loader);
    select_icd();
}

/// Open the Vulkan loader.
///
/// Order: the caller's `NVOC_VULKAN_LOADER` override, then the platform's own
/// loader, then the macOS GPU-bundle fallback resolved by [`prepare`].
pub fn load_entry() -> Result<ash::Entry, Box<dyn std::error::Error>> {
    if let Some(path) = env_path(LOADER_PATH_ENV) {
        // SAFETY: `dlopen`ing a native library is inherently unsafe; this one
        // was named explicitly by the caller for exactly this purpose.
        return unsafe { ash::Entry::load_from(&path) }.map_err(|err| {
            format!(
                "failed to load the Vulkan loader named by {}: {} ({err})",
                LOADER_PATH_ENV,
                path.display()
            )
            .into()
        });
    }
    // SAFETY: `dlopen`ing a native library is inherently unsafe. This is the
    // platform's own loader search, exactly as the callers used to do inline.
    match unsafe { ash::Entry::load() } {
        Ok(entry) => Ok(entry),
        Err(default_err) => {
            let bundled = RESOLVED_LOADER.get_or_init(resolve_bundled_loader).clone();
            let Some(path) = bundled else {
                return Err(Box::new(default_err));
            };
            // SAFETY: `dlopen`ing a native library is inherently unsafe; this
            // is the loader that ships with the macOS GPU driver bundle, and
            // it stays alive for as long as the returned `Entry` does.
            unsafe { ash::Entry::load_from(&path) }.map_err(|err| {
                format!(
                    "no Vulkan loader was found ({default_err}); the GPU-bundle loader at {} \
                     could not be opened either ({err}). Install a Vulkan loader, or point {} at \
                     one.",
                    path.display(),
                    LOADER_PATH_ENV
                )
                .into()
            })
        }
    }
}
