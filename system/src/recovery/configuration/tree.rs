//! Descriptor based configuration snapshots and private prepared tree writing.
//! Unsupported ACLs/xattrs and special files refuse before publication.
use super::{Content, Object, Tree, invalid, merge};
use crate::recovery::update::journal::open_directory;
use rustix::fs::{self, Mode, OFlags};
use std::{
    fs::File,
    io::{self, Read, Write},
    os::{
        fd::AsRawFd,
        unix::fs::{MetadataExt, PermissionsExt},
    },
    path::Path,
};

const MAX_FILE: usize = 16 * 1024 * 1024;
const MAX_TREE: usize = 64 * 1024 * 1024;
const MAX_OBJECTS: usize = 20_000;
const MAX_DEPTH: usize = 64;

fn proc_path(directory: &File, name: &str) -> String {
    format!("/proc/self/fd/{}/{name}", directory.as_raw_fd())
}

fn no_attributes(path: &str) -> io::Result<()> {
    let path = std::ffi::CString::new(path).map_err(invalid)?;
    // llistxattr inspects the object itself, including symlinks, without reading
    // secret values. A nonempty list needs an explicitly supported merge adapter.
    let count = unsafe { libc::llistxattr(path.as_ptr(), std::ptr::null_mut(), 0) };
    if count < 0 {
        return Err(io::Error::last_os_error());
    }
    if count != 0 {
        return Err(invalid(
            "Configuration ACLs/xattrs require a supported merge adapter",
        ));
    }
    Ok(())
}

fn snapshot_at(
    directory: &File,
    prefix: &str,
    depth: usize,
    bytes: &mut usize,
    result: &mut Tree,
) -> io::Result<()> {
    if depth > MAX_DEPTH {
        return Err(invalid("Configuration nesting exceeds its bound"));
    }
    for item in std::fs::read_dir(format!("/proc/self/fd/{}", directory.as_raw_fd()))? {
        if result.len() >= MAX_OBJECTS {
            return Err(invalid("Configuration object count exceeds its bound"));
        }
        let item = item?;
        let name = item
            .file_name()
            .into_string()
            .map_err(|_| invalid("Configuration name must be UTF8"))?;
        let relative = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let path = proc_path(directory, &name);
        let before = std::fs::symlink_metadata(&path)?;
        no_attributes(&path)?;
        let mode = before.mode() & 0o7777;
        let content = if before.is_dir() {
            let child = File::from(fs::openat(
                directory,
                name.as_str(),
                OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
                Mode::empty(),
            )?);
            let actual = child.metadata()?;
            if actual.dev() != before.dev() || actual.ino() != before.ino() {
                return Err(invalid("Configuration directory changed during open"));
            }
            result.insert(
                relative.clone(),
                Object {
                    content: Content::Directory,
                    uid: before.uid(),
                    gid: before.gid(),
                    mode,
                },
            );
            snapshot_at(&child, &relative, depth + 1, bytes, result)?;
            Content::Directory
        } else if before.file_type().is_symlink() {
            let target = std::fs::read_link(&path)?;
            let target = target
                .into_os_string()
                .into_string()
                .map_err(|_| invalid("Configuration link must be UTF8"))?;
            if target.len() > 4096 || target.contains('\0') {
                return Err(invalid("Configuration link exceeds its bound"));
            }
            *bytes = bytes
                .checked_add(target.len())
                .ok_or_else(|| invalid("Configuration size overflow"))?;
            Content::Symlink(target)
        } else if before.is_file() && before.nlink() == 1 && before.len() <= MAX_FILE as u64 {
            let file = File::from(fs::openat(
                directory,
                name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )?);
            let actual = file.metadata()?;
            if !actual.is_file()
                || actual.dev() != before.dev()
                || actual.ino() != before.ino()
                || actual.nlink() != 1
            {
                return Err(invalid("Configuration file changed during open"));
            }
            let mut contents = Vec::new();
            file.take(MAX_FILE as u64 + 1).read_to_end(&mut contents)?;
            *bytes = bytes
                .checked_add(contents.len())
                .ok_or_else(|| invalid("Configuration size overflow"))?;
            if contents.len() > MAX_FILE {
                return Err(invalid("Configuration file exceeds its bound"));
            }
            Content::File(contents)
        } else {
            return Err(invalid(
                "Configuration contains unsupported special or hard linked files",
            ));
        };
        if *bytes > MAX_TREE {
            return Err(invalid("Configuration bytes exceed their bound"));
        }
        let after = std::fs::symlink_metadata(&path)?;
        if before.dev() != after.dev()
            || before.ino() != after.ino()
            || before.len() != after.len()
            || before.mode() != after.mode()
            || before.uid() != after.uid()
            || before.gid() != after.gid()
            || before.mtime() != after.mtime()
            || before.mtime_nsec() != after.mtime_nsec()
            || before.ctime() != after.ctime()
            || before.ctime_nsec() != after.ctime_nsec()
        {
            return Err(invalid("Configuration changed during snapshot"));
        }
        result.insert(
            relative,
            Object {
                content,
                uid: before.uid(),
                gid: before.gid(),
                mode,
            },
        );
    }
    Ok(())
}

pub fn snapshot(path: &Path) -> io::Result<Tree> {
    let directory = open_directory(path)?;
    no_attributes(&proc_path(&directory, "."))?;
    let metadata = directory.metadata()?;
    let mut tree = Tree::from([(
        ".".into(),
        Object {
            content: Content::Directory,
            uid: metadata.uid(),
            gid: metadata.gid(),
            mode: metadata.mode() & 0o7777,
        },
    )]);
    snapshot_at(&directory, "", 0, &mut 0, &mut tree)?;
    super::validate(&tree)?;
    Ok(tree)
}

fn parent(root: &File, path: &str) -> io::Result<(File, String)> {
    let mut parts: Vec<_> = path.split('/').collect();
    let name = parts
        .pop()
        .ok_or_else(|| invalid("Missing prepared filename"))?
        .to_owned();
    let mut directory = root.try_clone()?;
    for part in parts {
        directory = File::from(fs::openat(
            &directory,
            part,
            OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
            Mode::empty(),
        )?);
    }
    Ok((directory, name))
}

fn write(root: &File, tree: &Tree) -> io::Result<()> {
    super::validate(tree)?;
    for (path, object) in tree {
        if path == "." {
            continue;
        }
        let (directory, name) = parent(root, path)?;
        match &object.content {
            Content::Directory => fs::mkdirat(
                &directory,
                name.as_str(),
                Mode::RUSR | Mode::WUSR | Mode::XUSR,
            )?,
            Content::File(bytes) => {
                let mut file = File::from(fs::openat(
                    &directory,
                    name.as_str(),
                    OFlags::WRONLY
                        | OFlags::CREATE
                        | OFlags::EXCL
                        | OFlags::NOFOLLOW
                        | OFlags::CLOEXEC,
                    Mode::RUSR | Mode::WUSR,
                )?);
                file.write_all(bytes)?;
                file.sync_all()?;
            }
            Content::Symlink(target) => fs::symlinkat(target.as_str(), &directory, name.as_str())?,
        }
    }
    // Restore directory permissions last, so mode 0500 parents do not prevent
    // writing their children. Numeric ownership is never translated by name.
    for (path, object) in tree.iter().rev() {
        let (directory, name) = parent(root, path)?;
        fs::chownat(
            &directory,
            name.as_str(),
            Some(rustix::process::Uid::from_raw(object.uid)),
            Some(rustix::process::Gid::from_raw(object.gid)),
            fs::AtFlags::SYMLINK_NOFOLLOW,
        )?;
        if !matches!(object.content, Content::Symlink(_)) {
            let file = File::from(fs::openat(
                &directory,
                name.as_str(),
                OFlags::RDONLY | OFlags::NOFOLLOW | OFlags::NONBLOCK | OFlags::CLOEXEC,
                Mode::empty(),
            )?);
            file.set_permissions(std::fs::Permissions::from_mode(object.mode))?;
            file.sync_all()?;
        }
        directory.sync_all()?;
    }
    root.sync_all()
}

/// Create a new private tree only. Caller holds the volume lock and has quiesced
/// the old runtime; no live /etc or selection is modified by this operation.
/// Native account validation/planning precedes this call. New account additions
/// must be applied offline with native tools and revalidated before selection.
pub fn prepare(base: &Path, current: &Path, target: &Path, destination: &Path) -> io::Result<()> {
    let base_tree = snapshot(base)?;
    let current_tree = snapshot(current)?;
    let target_tree = snapshot(target)?;
    let result = merge(&base_tree, &current_tree, &target_tree)?;
    let parent_path = destination
        .parent()
        .ok_or_else(|| invalid("Missing prepared directory parent"))?;
    let name = destination
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| invalid("Invalid prepared directory name"))?;
    if base == destination || current == destination || target == destination {
        return Err(invalid("Prepared tree must be separate from every input"));
    }
    let parent = open_directory(parent_path)?;
    fs::mkdirat(&parent, name, Mode::RUSR | Mode::WUSR | Mode::XUSR)?;
    let root = File::from(fs::openat(
        &parent,
        name,
        OFlags::RDONLY | OFlags::DIRECTORY | OFlags::NOFOLLOW | OFlags::CLOEXEC,
        Mode::empty(),
    )?);
    write(&root, &result)?;
    if snapshot(base)? != base_tree
        || snapshot(current)? != current_tree
        || snapshot(target)? != target_tree
        || snapshot(destination)? != result
    {
        return Err(invalid(
            "Configuration changed while preparing the new generation",
        ));
    }
    parent.sync_all()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        os::unix::fs::symlink,
        sync::atomic::{AtomicU64, Ordering},
    };
    #[test]
    fn prepared_tree_preserves_current_secrets_links_and_permissions() {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = std::env::current_dir()
            .unwrap()
            .join("target/configuration-tests")
            .join(format!(
                "{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o700)).unwrap();
        for name in ["base", "current", "target"] {
            let path = root.join(name);
            std::fs::create_dir(&path).unwrap();
            std::fs::write(path.join("ordinary"), b"old vendor").unwrap();
            std::fs::write(path.join("shadow"), b"old synthetic credential").unwrap();
            symlink("/outside/never-read", path.join("link")).unwrap();
        }
        std::fs::write(root.join("current/shadow"), b"new synthetic credential").unwrap();
        std::fs::set_permissions(
            root.join("current/shadow"),
            std::fs::Permissions::from_mode(0o600),
        )
        .unwrap();
        std::fs::write(root.join("target/ordinary"), b"new vendor").unwrap();
        prepare(
            &root.join("base"),
            &root.join("current"),
            &root.join("target"),
            &root.join("prepared"),
        )
        .unwrap();
        assert_eq!(
            std::fs::read(root.join("prepared/shadow")).unwrap(),
            b"new synthetic credential"
        );
        assert_eq!(
            std::fs::read(root.join("prepared/ordinary")).unwrap(),
            b"new vendor"
        );
        assert_eq!(
            std::fs::read_link(root.join("prepared/link")).unwrap(),
            std::path::PathBuf::from("/outside/never-read")
        );
        assert_eq!(
            std::fs::metadata(root.join("prepared/shadow"))
                .unwrap()
                .mode()
                & 0o777,
            0o600
        );
        assert_eq!(
            std::fs::metadata(root.join("prepared")).unwrap().mode() & 0o777,
            std::fs::metadata(root.join("current")).unwrap().mode() & 0o777
        );
        assert!(
            prepare(
                &root.join("base"),
                &root.join("current"),
                &root.join("target"),
                &root.join("prepared")
            )
            .is_err()
        );
        std::fs::remove_dir_all(root).unwrap();
    }
}
