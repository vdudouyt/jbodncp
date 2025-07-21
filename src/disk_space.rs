use libc::{statvfs, c_char};
use std::ffi::CString;
use std::mem::MaybeUninit;
use std::path::Path;

pub fn get_available_space(path: &Path) -> Option<u64> {
    let c_path = CString::new(path.to_string_lossy().as_bytes()).ok()?;
    let mut stat: MaybeUninit<statvfs> = MaybeUninit::uninit();

    unsafe {
        if statvfs(c_path.as_ptr() as *const c_char, stat.as_mut_ptr()) == 0 {
            let stat = stat.assume_init();
            Some(stat.f_bsize as u64 * stat.f_bavail as u64)
        } else {
            None
        }
    }
}
