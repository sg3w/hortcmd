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
use std::{
    cell::OnceCell,
    collections::HashMap,
    path::{Component, Path, PathBuf},
};

/// Metadata is reused only within one bounded worker batch, never across calls.
#[derive(Default)]
pub(super) struct Batch {
    mounts: OnceCell<Option<Vec<super::super::drives_native::Mount>>>,
    parents: HashMap<PathBuf, bool>,
}

impl Batch {
    fn local_icon_path<'a>(&mut self, request: &'a FileIconRequest) -> Option<&'a str> {
        self.local_icon_path_with(request, |path| {
            std::fs::symlink_metadata(path)
                .ok()
                .map(|meta| meta.file_type().is_symlink())
        })
    }

    fn local_icon_path_with<'a>(
        &mut self,
        request: &'a FileIconRequest,
        mut is_link: impl FnMut(&Path) -> Option<bool>,
    ) -> Option<&'a str> {
        if request.is_symlink {
            return None;
        }
        let raw = request.path.as_deref()?;
        let path = Path::new(raw);
        if !path.is_absolute() || path.components().any(|p| matches!(p, Component::ParentDir)) {
            return None;
        }
        // Lazily fetch the mount table once; type-only batches need no lookup.
        let mounts = self
            .mounts
            .get_or_init(super::super::drives_native::icon_mounts)
            .as_ref()?;
        let mount = mounts
            .iter()
            .filter(|m| path.starts_with(&m.drive.mount))
            .max_by_key(|m| m.drive.mount.len())?;
        if mount.remote {
            return None;
        }
        if let Some(parent) = path.parent() {
            let clean = self.parents.entry(parent.to_path_buf()).or_insert_with(|| {
                parent
                    .ancestors()
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .all(|ancestor| is_link(ancestor) == Some(false))
            });
            if !*clean {
                return None;
            }
        }
        // Always inspect the requested entry itself, including stale link flags.
        if is_link(path) != Some(false) {
            return None;
        }
        Some(raw)
    }
}

pub(super) fn render(request: &FileIconRequest, batch: &mut Batch) -> Option<String> {
    autoreleasepool(|_| {
        let workspace = NSWorkspace::sharedWorkspace();
        let image = if let Some(path) = batch.local_icon_path(request) {
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
    fn a_batch_reuses_ancestors_but_checks_each_leaf_and_refuses_remote_paths() {
        use super::super::super::{dir::Drive, drives_native::Mount};
        fn batch() -> Batch {
            let batch = Batch::default();
            let mounts = vec![
                Mount {
                    drive: Drive {
                        name: "local".into(),
                        mount: "/".into(),
                        total: 0,
                        free: 0,
                    },
                    remote: false,
                },
                Mount {
                    drive: Drive {
                        name: "remote".into(),
                        mount: "/remote".into(),
                        total: 0,
                        free: 0,
                    },
                    remote: true,
                },
            ];
            assert!(batch.mounts.set(Some(mounts)).is_ok());
            batch
        }
        let calls = std::cell::Cell::new(0);
        let mut stat = |path: &Path| {
            calls.set(calls.get() + 1);
            Some(path.ends_with("link"))
        };
        let mut batch_one = batch();
        let mut req = FileIconRequest {
            id: "test".into(),
            name: "a".into(),
            path: Some("/local/dir/a".into()),
            is_dir: false,
            is_symlink: false,
            pixels: 28,
        };
        assert!(batch_one.local_icon_path_with(&req, &mut stat).is_some());
        assert_eq!(calls.get(), 4);
        req.path = Some("/local/dir/b".into());
        assert!(batch_one.local_icon_path_with(&req, &mut stat).is_some());
        assert_eq!(calls.get(), 5);
        req.path = Some("/local/dir/link".into());
        assert!(batch_one.local_icon_path_with(&req, &mut stat).is_none());
        assert_eq!(calls.get(), 6);
        req.path = Some("/remote/file".into());
        assert!(batch_one.local_icon_path_with(&req, &mut stat).is_none());
        assert_eq!(calls.get(), 6);
        req.path = Some("/local/dir/a".into());
        assert!(batch().local_icon_path_with(&req, &mut stat).is_some());
        assert_eq!(calls.get(), 10);
        req.path = Some("/local/link/file".into());
        assert!(batch_one.local_icon_path_with(&req, &mut stat).is_none());
    }

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
        let before = render(&request, &mut Batch::default()).unwrap();
        let workspace = NSWorkspace::sharedWorkspace();
        let custom =
            workspace.iconForFile(&NSString::from_str("/System/Applications/TextEdit.app"));
        assert!(workspace.setIcon_forFile_options(
            Some(&custom),
            &NSString::from_str(file.to_str().unwrap()),
            objc2_app_kit::NSWorkspaceIconCreationOptions::empty()
        ));
        let after = render(&request, &mut Batch::default()).unwrap();
        assert_ne!(before, after, "individual icon must be honored");
        let mut link = request.clone();
        link.is_symlink = true;
        assert!(Batch::default().local_icon_path(&link).is_none());
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
                total += render(&req, &mut Batch::default()).unwrap().len();
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
            let image = render(&req, &mut Batch::default()).expect("native icon");
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
