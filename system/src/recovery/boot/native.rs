//! Native mount/loop operations used only after the fixed layout 2 boot selector.
use super::*;
use std::os::unix::fs::symlink;

fn child(parent: &File, name: &str) -> io::Result<File> {
    match fs::mkdirat(parent, name, Mode::RUSR | Mode::WUSR | Mode::XUSR) {
        Ok(()) | Err(rustix::io::Errno::EXIST) => {}
        Err(error) => return Err(error.into()),
    }
    let file = File::from(fs::openat(
        parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let metadata = file.metadata()?;
    if metadata.uid() != 0 || metadata.mode() & 0o077 != 0 {
        return Err(invalid("Early boot workspace must be root private"));
    }
    Ok(file)
}

pub(super) fn runtime_directories() -> io::Result<()> {
    let run = open_directory(Path::new("/run"))?;
    let linos = child(&run, "linos")?;
    child(&linos, "volume")?;
    child(&linos, "image")?;
    child(&open_directory(Path::new("/"))?, "efi")?;
    Ok(())
}

fn mount(
    source: &str,
    target: &str,
    kind: Option<&str>,
    flags: libc::c_ulong,
    options: &str,
) -> io::Result<()> {
    let source = CString::new(source).map_err(|_| invalid("Invalid native mount source"))?;
    let target = CString::new(target).map_err(|_| invalid("Invalid native mount target"))?;
    let kind = kind
        .map(CString::new)
        .transpose()
        .map_err(|_| invalid("Invalid native filesystem type"))?;
    let options = CString::new(options).map_err(|_| invalid("Invalid native mount options"))?;
    // Fixed managed paths and retained block descriptors only. No shell parsing
    // or util-linux pathname canonicalization occurs between inspection/mount.
    let result = unsafe {
        libc::mount(
            source.as_ptr(),
            target.as_ptr(),
            kind.as_ref()
                .map_or(std::ptr::null(), |value| value.as_ptr()),
            flags,
            options.as_ptr().cast(),
        )
    };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn mount_file(
    file: &File,
    target: &str,
    kind: &str,
    flags: libc::c_ulong,
    options: &str,
) -> io::Result<()> {
    mount(
        &format!("/proc/{}/fd/{}", std::process::id(), file.as_raw_fd()),
        target,
        Some(kind),
        flags,
        options,
    )
}

pub(super) fn bind(source: &str, target: &str, readonly: bool) -> io::Result<()> {
    for path in [source, target] {
        if std::fs::symlink_metadata(path)?.file_type().is_symlink() {
            return Err(invalid("Managed bind endpoints cannot be symbolic links"));
        }
    }
    mount(source, target, None, libc::MS_BIND, "")?;
    if readonly {
        mount(
            "none",
            target,
            None,
            libc::MS_BIND | libc::MS_REMOUNT | libc::MS_RDONLY,
            "",
        )?;
    }
    Ok(())
}

pub(super) fn move_mount(source: &str, target: &str) -> io::Result<()> {
    mount(source, target, None, libc::MS_MOVE, "")
}

#[repr(C)]
struct LoopInfo {
    device: u64,
    inode: u64,
    rdevice: u64,
    offset: u64,
    size_limit: u64,
    number: u32,
    encrypt_type: u32,
    encrypt_key_size: u32,
    flags: u32,
    file_name: [u8; 64],
    crypt_name: [u8; 64],
    encrypt_key: [u8; 32],
    init: [u64; 2],
}
#[repr(C)]
struct LoopConfig {
    fd: u32,
    block_size: u32,
    info: LoopInfo,
    reserved: [u64; 8],
}
const _: () =
    assert!(std::mem::size_of::<LoopInfo>() == 232 && std::mem::size_of::<LoopConfig>() == 304);

pub(super) fn attach_readonly(image: &File) -> io::Result<()> {
    let control = File::from(fs::open(
        "/dev/loop-control",
        OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    // LOOP_CTL_GET_FREE returns a kernel allocated free loop number.
    let number = unsafe { libc::ioctl(control.as_raw_fd(), 0x4c82) };
    if number < 0 {
        return Err(io::Error::last_os_error());
    }
    if number > 65535 {
        return Err(invalid("Native loop number exceeds its bound"));
    }
    let path = format!("/dev/loop{number}");
    let device = File::from(fs::open(
        path.as_str(),
        OFlags::RDWR | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    let metadata = device.metadata()?;
    if !metadata.file_type().is_block_device()
        || rustix::fs::major(metadata.rdev()) != 7
        || rustix::fs::minor(metadata.rdev()) != number as u32
    {
        return Err(invalid("Native loop node identity is inconsistent"));
    }
    // Zero all unused Linux UAPI fields. No offset, limit or encryption. Keep
    // the attachment across local-top exit until the ordinary root mount opens
    // it; AUTOCLEAR here would detach before initramfs mounts the root.
    let mut config: LoopConfig = unsafe { std::mem::zeroed() };
    config.fd = image.as_raw_fd() as u32;
    config.info.flags = 1;
    let result = unsafe { libc::ioctl(device.as_raw_fd(), 0x4c0a, &config) };
    if result < 0 {
        return Err(io::Error::last_os_error());
    }
    if let Err(error) = symlink(&path, "/dev/linos-system") {
        let _ = unsafe { libc::ioctl(device.as_raw_fd(), 0x4c01) };
        return Err(error);
    }
    Ok(())
}
