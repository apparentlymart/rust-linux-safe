//! Lightweight but safe abstractions around Linux system calls related to
//! file descriptors.
//!
//! Implements standard library I/O traits by default, but can be made friendly
//! to `no_std` environments by disabling the default feature `std`.
#![no_std]

/// An encapsulated Linux file descriptor.
pub struct File {
    fd: linux_unsafe::int,
}

use linux_unsafe::raw::V;

impl File {
    #[inline]
    pub unsafe fn from_raw_fd(fd: linux_unsafe::int) -> Self {
        File { fd }
    }

    #[inline]
    pub fn create_raw(path: &[u8], mode: linux_unsafe::mode_t) -> Result<Self> {
        let path_raw = path.as_ptr() as *const linux_unsafe::char;
        let result = unsafe { linux_unsafe::creat(path_raw, mode as linux_unsafe::mode_t) };
        linux_unsafe::raw::unpack_standard_result(result as V)
            .map(|fd| unsafe { Self::from_raw_fd(fd as linux_unsafe::int) })
            .map_err(|e| e.into())
    }

    #[inline]
    pub fn open_raw(
        path: &[u8],
        flags: linux_unsafe::int,
        mode: linux_unsafe::mode_t,
    ) -> Result<Self> {
        let path_raw = path.as_ptr() as *const linux_unsafe::char;
        let result = unsafe {
            linux_unsafe::open(
                path_raw,
                flags as linux_unsafe::int,
                mode as linux_unsafe::mode_t,
            )
        };
        linux_unsafe::raw::unpack_standard_result(result as V)
            .map(|fd| unsafe { Self::from_raw_fd(fd as linux_unsafe::int) })
            .map_err(|e| e.into())
    }

    #[inline]
    pub fn close(mut self) -> Result<()> {
        unsafe { self.close_mut() }
    }

    /// Closes the underlying file descriptor without consuming it.
    ///
    /// Safety: Callers must not use the file object again after calling this
    /// method, because the file descriptor will either be dangling or will
    /// be referring to some other unrelated file.
    #[inline(always)]
    pub unsafe fn close_mut(&mut self) -> Result<()> {
        let result = unsafe { linux_unsafe::close(self.fd) };
        linux_unsafe::raw::unpack_standard_result(result as V)
            .map(|_| ())
            .map_err(|e| e.into())
    }

    #[inline]
    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        let buf_ptr = buf.as_mut_ptr() as *mut linux_unsafe::void;
        let buf_size = buf.len();
        let result = unsafe { linux_unsafe::read(self.fd, buf_ptr, buf_size) };
        linux_unsafe::raw::unpack_standard_result(result as V)
            .map(|v| v as usize)
            .map_err(|e| e.into())
    }

    #[inline]
    pub fn seek(&mut self, pos: impl Into<SeekFrom>) -> Result<u64> {
        let pos = pos.into();
        let raw_offs = pos.for_raw_offset();

        #[cfg(not(target_pointer_width = "32"))]
        {
            // For 64-bit platforms we can just use lseek, because off_t is
            // bit enough for all offsets.
            let raw_whence = pos.for_raw_whence();
            let result = unsafe { linux_unsafe::lseek(self.fd, raw_offs, raw_whence) };
            linux_unsafe::raw::unpack_standard_result(result as V)
                .map(|v| v as u64)
                .map_err(|e| e.into())
        }

        #[cfg(target_pointer_width = "32")]
        {
            // For 32-bit platforms we need to use _llseek instead, which
            // splits the offset across two arguments.
            let raw_offs_high = ((raw_offs as u64) >> 32) as linux_unsafe::ulong;
            let raw_offs_low = (raw_offs as u64) as linux_unsafe::ulong;
            use core::cell::UnsafeCell;
            let result: UnsafeCell<linux_unsafe::loff_t> = UnsafeCell::new(0);
            let result_ptr = result.get();
            let raw_whence = pos.for_raw_uwhence();
            let status = unsafe {
                linux_unsafe::_llseek(self.fd, raw_offs_high, raw_offs_low, result_ptr, raw_whence)
            };
            match linux_unsafe::raw::unpack_standard_result(status as V) {
                Ok(_) => {
                    let result_offs = unsafe { *result_ptr } as u64;
                    Ok(result_offs)
                }
                Err(e) => Err(e.into()),
            }
        }
    }

    #[inline]
    pub fn sync(&mut self) -> Result<()> {
        let result = unsafe { linux_unsafe::syncfs(self.fd) };
        linux_unsafe::raw::unpack_standard_result(result as V)
            .map(|_| ())
            .map_err(|e| e.into())
    }

    #[inline]
    pub fn write(&mut self, buf: &[u8]) -> Result<usize> {
        let buf_ptr = buf.as_ptr() as *const linux_unsafe::void;
        let buf_size = buf.len();
        let result = unsafe { linux_unsafe::write(self.fd, buf_ptr, buf_size) };
        linux_unsafe::raw::unpack_standard_result(result as V)
            .map(|v| v as usize)
            .map_err(|e| e.into())
    }
}

impl Drop for File {
    /// Attempts to close the file when it's no longer in scope.
    ///
    /// This implicit close ignores errors, which might cause data loss if
    /// the final commit of data to disk fails. Use [`File::close`] explicitly
    /// if you need to detect errors.
    #[allow(unused_must_use)] // intentionally discarding close result
    fn drop(&mut self) {
        unsafe { self.close_mut() };
    }
}

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
impl std::io::Read for File {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        self.read(buf).map_err(|e| e.into())
    }
}

#[cfg(feature = "std")]
impl std::io::Write for File {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.write(buf).map_err(|e| e.into())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.sync().map_err(|e| e.into())
    }
}

#[cfg(feature = "std")]
impl std::io::Seek for File {
    fn seek(&mut self, pos: std::io::SeekFrom) -> std::io::Result<u64> {
        self.seek(pos).map_err(|e| e.into())
    }
}

#[cfg(feature = "std")]
impl std::os::fd::FromRawFd for File {
    unsafe fn from_raw_fd(fd: std::os::fd::RawFd) -> Self {
        Self {
            fd: fd as linux_unsafe::int,
        }
    }
}

#[cfg(feature = "std")]
impl std::os::fd::IntoRawFd for File {
    fn into_raw_fd(self) -> std::os::fd::RawFd {
        self.fd as std::os::fd::RawFd
    }
}

/// Represents a result from a kernel call that might fail.
pub type Result<T> = core::result::Result<T, Error>;

/// Represents an error code directly from the kernel.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
#[repr(transparent)]
pub struct Error(pub i32);

impl Error {
    #[inline(always)]
    pub const fn new(raw: i32) -> Self {
        Self(raw)
    }

    #[cfg(feature = "std")]
    #[inline(always)]
    pub fn into_std_io_error(self) -> std::io::Error {
        std::io::Error::from_raw_os_error(self.0)
    }
}

impl From<i32> for Error {
    #[inline(always)]
    fn from(value: i32) -> Self {
        Self::new(value)
    }
}

#[cfg(feature = "std")]
impl Into<std::io::Error> for Error {
    #[inline(always)]
    fn into(self) -> std::io::Error {
        self.into_std_io_error()
    }
}

/// Used with [`File::seek`] to specify the starting point and offset.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

impl SeekFrom {
    #[inline]
    const fn for_raw_offset(self) -> linux_unsafe::loff_t {
        match self {
            SeekFrom::Start(v) => v as linux_unsafe::loff_t,
            SeekFrom::End(v) => v as linux_unsafe::loff_t,
            SeekFrom::Current(v) => v as linux_unsafe::loff_t,
        }
    }

    #[inline]
    const fn for_raw_whence(self) -> linux_unsafe::int {
        match self {
            SeekFrom::Start(_) => linux_unsafe::SEEK_SET,
            SeekFrom::End(_) => linux_unsafe::SEEK_END,
            SeekFrom::Current(_) => linux_unsafe::SEEK_CUR,
        }
    }

    #[allow(dead_code)] // only used on 32-bit platforms
    #[inline]
    const fn for_raw_uwhence(self) -> linux_unsafe::uint {
        self.for_raw_whence() as linux_unsafe::uint
    }
}

#[cfg(feature = "std")]
impl From<std::io::SeekFrom> for SeekFrom {
    fn from(value: std::io::SeekFrom) -> Self {
        match value {
            std::io::SeekFrom::Start(v) => Self::Start(v),
            std::io::SeekFrom::End(v) => Self::End(v),
            std::io::SeekFrom::Current(v) => Self::Current(v),
        }
    }
}

#[cfg(test)]
mod tests;
