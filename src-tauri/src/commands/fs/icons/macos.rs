//! NSWorkspace lookup and offscreen drawing on the single icon worker.
//! Workspace documents icon lookup as thread-safe; the bitmap graphics context
//! is private to this thread. No shared NSImage is resized or mutated.
use super::FileIconRequest;
use base64::{engine::general_purpose::STANDARD, Engine};
use objc2::{rc::autoreleasepool, AnyThread};
use objc2_app_kit::{
    NSBitmapImageFileType, NSBitmapImageRep, NSDeviceRGBColorSpace, NSGraphicsContext, NSWorkspace,
};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString};
use std::path::{Component, Path};

fn local_icon_path(request: &FileIconRequest) -> Option<&str> {
    if request.is_symlink {
        return None;
    }
    let raw = request.path.as_deref()?;
    let path = Path::new(raw);
    if !path.is_absolute() || path.components().any(|p| matches!(p, Component::ParentDir)) {
        return None;
    }
    // Cached kernel mount metadata; never contact a network mount for its icon.
    let mounts = super::super::drives_native::icon_mounts()?;
    let mount = mounts
        .iter()
        .filter(|m| path.starts_with(&m.drive.mount))
        .max_by_key(|m| m.drive.mount.len())?;
    if mount.remote {
        return None;
    }
    // Do not resolve links, even if the frontend's listing metadata is stale.
    for ancestor in path.ancestors().collect::<Vec<_>>().into_iter().rev() {
        if std::fs::symlink_metadata(ancestor)
            .ok()?
            .file_type()
            .is_symlink()
        {
            return None;
        }
    }
    Some(raw)
}

pub(super) fn render(request: &FileIconRequest) -> Option<String> {
    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let image = if let Some(path) = local_icon_path(request) {
            workspace.iconForFile(&NSString::from_str(path))
        } else {
            let kind = if request.is_dir {
                "public.folder"
            } else {
                Path::new(&request.name)
                    .extension()
                    .and_then(|s| s.to_str())
                    .unwrap_or("public.data")
            };
            // Supports the app's pre-macOS-11 deployment target. The replacement
            // iconForContentType requires macOS 11 and UniformTypeIdentifiers.
            #[allow(deprecated)]
            workspace.iconForFileType(&NSString::from_str(kind))
        };
        let pixels = request.pixels as isize;
        // Null planes asks AppKit to allocate/own the bounded RGBA buffer.
        let bitmap = unsafe {
            NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
                NSBitmapImageRep::alloc(), std::ptr::null_mut(), pixels, pixels, 8, 4,
                true, false, NSDeviceRGBColorSpace, pixels * 4, 32,
            )?
        };
        let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap)?;
        NSGraphicsContext::saveGraphicsState_class();
        NSGraphicsContext::setCurrentContext(Some(&context));
        image.drawInRect(NSRect::new(
            NSPoint::new(0., 0.),
            NSSize::new(pixels as f64, pixels as f64),
        ));
        NSGraphicsContext::restoreGraphicsState_class();
        // Empty, correctly typed property dictionary; PNG needs no options.
        let data = unsafe {
            bitmap.representationUsingType_properties(
                NSBitmapImageFileType::PNG,
                &NSDictionary::new(),
            )?
        };
        let bytes = data.to_vec();
        (bytes.len() <= 96 * 1024)
            .then(|| format!("data:image/png;base64,{}", STANDARD.encode(bytes)))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    #[ignore = "native Workspace custom-icon integration and benchmark; macOS only"]
    fn native_custom_and_benchmark() {
        let root = std::env::temp_dir().join(format!("hortcmd-icons-{}", std::process::id()));
        std::fs::create_dir(&root).unwrap();
        struct Cleanup(std::path::PathBuf);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = std::fs::remove_dir_all(&self.0);
            }
        }
        let _cleanup = Cleanup(root.clone());
        let root = root.canonicalize().unwrap();
        let file = root.join("custom.txt");
        std::fs::write(&file, b"icon test").unwrap();
        let request = FileIconRequest {
            id: "custom".into(),
            name: "custom.txt".into(),
            path: Some(file.to_str().unwrap().into()),
            is_dir: false,
            is_symlink: false,
            pixels: 80,
        };
        let before = render(&request).unwrap();
        let workspace = NSWorkspace::sharedWorkspace();
        let custom =
            workspace.iconForFile(&NSString::from_str("/System/Applications/TextEdit.app"));
        assert!(workspace.setIcon_forFile_options(
            Some(&custom),
            &NSString::from_str(file.to_str().unwrap()),
            objc2_app_kit::NSWorkspaceIconCreationOptions::empty()
        ));
        let after = render(&request).unwrap();
        assert_ne!(before, after, "individual icon must be honored");
        let mut link = request.clone();
        link.is_symlink = true;
        assert!(local_icon_path(&link).is_none());
        if let Ok(output) = std::env::var("HORTCMD_ICON_PROBE_OUTPUT") {
            std::fs::write(
                output,
                STANDARD.decode(after.split_once(',').unwrap().1).unwrap(),
            )
            .unwrap();
        }
        for pass in 0..2 {
            let started = std::time::Instant::now();
            let mut total = 0;
            for index in 0..96 {
                let mut req = request.clone();
                req.name = format!(
                    "entry{index}.{}",
                    ["txt", "rs", "pdf", "png", "zip", "unknownextension"][index % 6]
                );
                let path = root.join(&req.name);
                std::fs::write(&path, []).unwrap();
                req.path = Some(path.to_str().unwrap().into());
                req.pixels = 28;
                total += render(&req).unwrap().len();
            }
            eprintln!(
                "96 local icons pass {pass}: {:?}, {total} transport bytes",
                started.elapsed()
            );
        }
    }
    #[test]
    #[ignore = "native Workspace rendering; run locally on macOS"]
    fn native_png_sizes_and_timing() {
        let start = std::time::Instant::now();
        for pixels in [14, 28, 40, 80, 128] {
            let req = FileIconRequest {
                id: "probe".into(),
                name: "probe.txt".into(),
                path: None,
                is_dir: false,
                is_symlink: false,
                pixels,
            };
            let image = render(&req).expect("native icon");
            let bytes = STANDARD.decode(image.split_once(',').unwrap().1).unwrap();
            assert_eq!(&bytes[..8], b"\x89PNG\r\n\x1a\n");
            assert_eq!(
                u32::from_be_bytes(bytes[16..20].try_into().unwrap()),
                u32::from(pixels)
            );
        }
        eprintln!("Five native sizes: {:?}", start.elapsed());
    }
}
