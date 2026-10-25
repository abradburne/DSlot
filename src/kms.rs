//! DSlot's own hold on the screens, straight through KMS, without SDL: a picture for each
//! screen, copied into a dumb buffer and set on the CRTC the console already lights that panel
//! with. At start-up it shows the skeleton within milliseconds, while SDL and GL take their
//! time; DSlot then stops being DRM master so that SDL can take the outputs.
//!
//! The card stays open for DSlot's whole life. Were it ever closed, with SDL's windows gone
//! and no game yet (or no more) on screen, the kernel would put the text console back on the
//! panels until someone else took them ("lastclose"). And between the shelf and a game, and
//! back, DSlot takes the outputs again for a moment to show the shelf's last frame, so the
//! screens never go to the console, nor black.

#[cfg(target_os = "linux")]
pub use linux::Kms;

/// Elsewhere (a desktop preview) there are no KMS outputs to take.
#[cfg(not(target_os = "linux"))]
pub struct Kms;

#[cfg(not(target_os = "linux"))]
impl Kms {
    pub fn open(_swap: bool) -> Result<Kms, String> {
        Err("KMS output is only on Linux".into())
    }
    pub fn show(&mut self, _imgs: &[&crate::gfx::Img]) -> Result<(), String> {
        Ok(())
    }
    pub fn paint(&mut self, _imgs: &[&crate::gfx::Img]) {}
    pub fn set(&self) -> Result<(), String> {
        Ok(())
    }
    pub fn take(&self) -> bool {
        false
    }
    pub fn release(&self) {}
}

#[cfg(target_os = "linux")]
mod linux {
    use crate::gfx::Img;
    use std::os::raw::c_ulong;

    /// The DRM ioctls DSlot uses, and their argument structs, from <drm/drm.h> and
    /// <drm/drm_mode.h>.
    const fn iowr(nr: u32, size: usize) -> c_ulong {
        ((3u32 << 30) | ((size as u32) << 16) | (0x64 << 8) | nr) as c_ulong
    }
    const GETRESOURCES: c_ulong = iowr(0xA0, std::mem::size_of::<CardRes>());
    const SETCRTC: c_ulong = iowr(0xA2, std::mem::size_of::<Crtc>());
    const GETENCODER: c_ulong = iowr(0xA6, std::mem::size_of::<GetEncoder>());
    const GETCONNECTOR: c_ulong = iowr(0xA7, std::mem::size_of::<GetConnector>());
    const ADDFB: c_ulong = iowr(0xAE, std::mem::size_of::<FbCmd>());
    const CREATE_DUMB: c_ulong = iowr(0xB2, std::mem::size_of::<CreateDumb>());
    const MAP_DUMB: c_ulong = iowr(0xB3, std::mem::size_of::<MapDumb>());
    const SET_MASTER: c_ulong = (0x64 << 8) | 0x1e;
    const DROP_MASTER: c_ulong = (0x64 << 8) | 0x1f;
    const CONNECTED: u32 = 1;

    #[repr(C)]
    #[derive(Default)]
    struct CardRes {
        fb_id_ptr: u64,
        crtc_id_ptr: u64,
        connector_id_ptr: u64,
        encoder_id_ptr: u64,
        count_fbs: u32,
        count_crtcs: u32,
        count_connectors: u32,
        count_encoders: u32,
        min_width: u32,
        max_width: u32,
        min_height: u32,
        max_height: u32,
    }

    #[repr(C)]
    #[derive(Default, Clone, Copy)]
    struct ModeInfo {
        clock: u32,
        hdisplay: u16,
        hsync_start: u16,
        hsync_end: u16,
        htotal: u16,
        hskew: u16,
        vdisplay: u16,
        vsync_start: u16,
        vsync_end: u16,
        vtotal: u16,
        vscan: u16,
        vrefresh: u32,
        flags: u32,
        kind: u32,
        name: [u8; 32],
    }

    #[repr(C)]
    #[derive(Default)]
    struct GetConnector {
        encoders_ptr: u64,
        modes_ptr: u64,
        props_ptr: u64,
        prop_values_ptr: u64,
        count_modes: u32,
        count_props: u32,
        count_encoders: u32,
        encoder_id: u32,
        connector_id: u32,
        connector_type: u32,
        connector_type_id: u32,
        connection: u32,
        mm_width: u32,
        mm_height: u32,
        subpixel: u32,
        pad: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct GetEncoder {
        encoder_id: u32,
        encoder_type: u32,
        crtc_id: u32,
        possible_crtcs: u32,
        possible_clones: u32,
    }

    #[repr(C)]
    #[derive(Default)]
    struct Crtc {
        set_connectors_ptr: u64,
        count_connectors: u32,
        crtc_id: u32,
        fb_id: u32,
        x: u32,
        y: u32,
        gamma_size: u32,
        mode_valid: u32,
        mode: ModeInfo,
    }

    #[repr(C)]
    #[derive(Default)]
    struct CreateDumb {
        height: u32,
        width: u32,
        bpp: u32,
        flags: u32,
        handle: u32,
        pitch: u32,
        size: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct MapDumb {
        handle: u32,
        pad: u32,
        offset: u64,
    }

    #[repr(C)]
    #[derive(Default)]
    struct FbCmd {
        fb_id: u32,
        width: u32,
        height: u32,
        pitch: u32,
        bpp: u32,
        depth: u32,
        handle: u32,
    }

    fn ioctl<T>(fd: i32, req: c_ulong, arg: &mut T) -> Result<(), String> {
        // SAFETY: every request here is paired with the struct the kernel expects for it.
        let r = unsafe { libc::ioctl(fd, req as _, arg as *mut T) };
        if r < 0 {
            return Err(format!(
                "DRM ioctl {req:#x}: {}",
                std::io::Error::last_os_error()
            ));
        }
        Ok(())
    }

    /// A panel: its connector, the CRTC lighting it, its mode, and a buffer to show.
    struct Screen {
        crtc: u32,
        connector: u32,
        mode: ModeInfo,
        fb: u32,
        map: *mut u8,
        size: usize,
        pitch: u32,
    }

    pub struct Kms {
        fd: i32,
        /// Top screen first.
        screens: Vec<Screen>,
    }

    impl Kms {
        /// Takes the first two connected outputs of the first KMS card, on the CRTCs they are
        /// already lit by (the console's). The first is the bottom screen unless `swap`.
        pub fn open(swap: bool) -> Result<Kms, String> {
            let (fd, connectors) = (0..4).find_map(card).ok_or("no KMS card")?;
            let mut kms = Kms {
                fd,
                screens: Vec::new(),
            };
            for id in connectors {
                if let Some(screen) = kms.screen(id)? {
                    kms.screens.push(screen);
                }
                if kms.screens.len() == 2 {
                    break;
                }
            }
            if kms.screens.is_empty() {
                return Err("no lit output".into());
            }
            // The first output (DSI-1) is the bottom, touch panel.
            if !swap {
                kms.screens.reverse();
            }
            Ok(kms)
        }

        /// The connector `id` as a screen, with a buffer of its own, if it is lit.
        fn screen(&self, id: u32) -> Result<Option<Screen>, String> {
            let fd = self.fd;
            let mut c = GetConnector {
                connector_id: id,
                ..Default::default()
            };
            ioctl(fd, GETCONNECTOR, &mut c)?;
            if c.connection != CONNECTED || c.count_modes == 0 || c.encoder_id == 0 {
                return Ok(None);
            }
            let mut modes = vec![ModeInfo::default(); c.count_modes as usize];
            let mut with_modes = GetConnector {
                connector_id: id,
                modes_ptr: modes.as_mut_ptr() as u64,
                count_modes: c.count_modes,
                ..Default::default()
            };
            ioctl(fd, GETCONNECTOR, &mut with_modes)?;
            let mut e = GetEncoder {
                encoder_id: c.encoder_id,
                ..Default::default()
            };
            ioctl(fd, GETENCODER, &mut e)?;
            if e.crtc_id == 0 {
                return Ok(None);
            }
            let mode = modes[0];
            let (width, height) = (mode.hdisplay as u32, mode.vdisplay as u32);
            let mut d = CreateDumb {
                width,
                height,
                bpp: 32,
                ..Default::default()
            };
            ioctl(fd, CREATE_DUMB, &mut d)?;
            let mut f = FbCmd {
                width,
                height,
                pitch: d.pitch,
                bpp: 32,
                depth: 24,
                handle: d.handle,
                ..Default::default()
            };
            ioctl(fd, ADDFB, &mut f)?;
            let mut m = MapDumb {
                handle: d.handle,
                ..Default::default()
            };
            ioctl(fd, MAP_DUMB, &mut m)?;
            let (prot, flags) = (libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED);
            // SAFETY: maps the dumb buffer the kernel just made, at the offset it gave for it.
            let map = unsafe {
                libc::mmap(
                    std::ptr::null_mut(),
                    d.size as usize,
                    prot,
                    flags,
                    fd,
                    m.offset as libc::off_t,
                )
            };
            if map == libc::MAP_FAILED {
                return Err("mmap of a dumb buffer failed".into());
            }
            eprintln!(
                "dslot: KMS connector {id} on CRTC {} {width}x{height}",
                e.crtc_id
            );
            Ok(Some(Screen {
                crtc: e.crtc_id,
                connector: id,
                mode,
                fb: f.fb_id,
                map: map.cast(),
                size: d.size as usize,
                pitch: d.pitch,
            }))
        }

        /// Shows one picture on each screen (top first), taking the outputs from the console.
        pub fn show(&mut self, imgs: &[&Img]) -> Result<(), String> {
            self.paint(imgs);
            self.set()
        }

        /// Copies one picture into each screen's buffer (top first), without showing them: it
        /// shows wherever the buffer is on screen, or will be.
        pub fn paint(&mut self, imgs: &[&Img]) {
            for (s, img) in self.screens.iter().zip(imgs) {
                let (w, h) = (
                    (s.mode.hdisplay as usize).min(img.w),
                    (s.mode.vdisplay as usize).min(img.h),
                );
                for y in 0..h {
                    // SAFETY: row `y` of the mapped buffer, `pitch` bytes apart, `w` pixels of 4
                    // bytes each, within the mode's size the buffer was made at.
                    let row = unsafe {
                        std::slice::from_raw_parts_mut(s.map.add(y * s.pitch as usize), w * 4)
                    };
                    for (x, px) in row.chunks_exact_mut(4).enumerate() {
                        let p = img.get(x, y);
                        px.copy_from_slice(&[p[2], p[1], p[0], 255]);
                    }
                }
            }
        }

        /// Puts each screen's buffer on its output. Needs DRM master.
        pub fn set(&self) -> Result<(), String> {
            for s in &self.screens {
                let mut connector = s.connector;
                let mut c = Crtc {
                    set_connectors_ptr: &mut connector as *mut u32 as u64,
                    count_connectors: 1,
                    crtc_id: s.crtc,
                    fb_id: s.fb,
                    mode_valid: 1,
                    mode: s.mode,
                    ..Default::default()
                };
                ioctl(self.fd, SETCRTC, &mut c)?;
            }
            Ok(())
        }

        /// Becomes DRM master again, to put a picture up while no one else has the outputs
        /// (between the shelf and a game). The kernel allows it for a card opened while no one
        /// was master, by the same process. Returns whether it is master now.
        pub fn take(&self) -> bool {
            // SAFETY: SET_MASTER takes no argument.
            unsafe { libc::ioctl(self.fd, SET_MASTER as _) == 0 }
        }

        /// Stops being DRM master, so that another opener (SDL) can set the outputs, while
        /// what is on screen stays there.
        pub fn release(&self) {
            // SAFETY: DROP_MASTER takes no argument.
            unsafe { libc::ioctl(self.fd, DROP_MASTER as _) };
        }
    }

    /// Opens /dev/dri/card`i` and lists its connectors, if it is a KMS card.
    fn card(i: u32) -> Option<(i32, Vec<u32>)> {
        let path = std::ffi::CString::new(format!("/dev/dri/card{i}")).ok()?;
        // SAFETY: a NUL-terminated path.
        let fd = unsafe { libc::open(path.as_ptr(), libc::O_RDWR | libc::O_CLOEXEC) };
        if fd < 0 {
            return None;
        }
        let mut res = CardRes::default();
        if ioctl(fd, GETRESOURCES, &mut res).is_ok() && res.count_connectors > 0 {
            let mut ids = vec![0u32; res.count_connectors as usize];
            let mut listed = CardRes {
                connector_id_ptr: ids.as_mut_ptr() as u64,
                count_connectors: res.count_connectors,
                ..Default::default()
            };
            if ioctl(fd, GETRESOURCES, &mut listed).is_ok() {
                return Some((fd, ids));
            }
        }
        // SAFETY: our own descriptor, not used again.
        unsafe { libc::close(fd) };
        None
    }

    impl Drop for Kms {
        fn drop(&mut self) {
            // Closing the card frees the buffers; whoever is master now keeps the outputs.
            for s in &self.screens {
                // SAFETY: the mapping made in `screen`, not used again.
                unsafe { libc::munmap(s.map.cast(), s.size) };
            }
            // SAFETY: our own descriptor, not used again.
            unsafe { libc::close(self.fd) };
        }
    }
}
