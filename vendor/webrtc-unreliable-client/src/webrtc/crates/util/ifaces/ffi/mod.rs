#[cfg(target_family = "windows")]
mod windows;
#[cfg(target_family = "windows")]
pub(crate) use self::windows::ifaces;

#[cfg(target_family = "unix")]
mod unix;
#[cfg(target_family = "unix")]
pub(crate) use self::unix::ifaces;
