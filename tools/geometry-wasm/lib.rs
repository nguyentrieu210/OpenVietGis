// Standalone, dependency-free geometry kernel compiled with pinned Rust 1.85.1.
// WGS84 spherical estimates deliberately match the existing frontend contract.
const R: f64 = 6_371_008.8;
const RAD: f64 = std::f64::consts::PI / 180.0;
const MAX_POINTS: usize = 2_000_000;

#[no_mangle]
pub extern "C" fn alloc_path(points: usize) -> *mut f64 {
    if points > MAX_POINTS { return std::ptr::null_mut(); }
    Box::into_raw(vec![0.0; points * 2 + 2].into_boxed_slice()) as *mut f64
}

/// # Safety
/// ptr must originate from alloc_path(points) in this module and be used once.
#[no_mangle]
pub unsafe extern "C" fn free_path(ptr: *mut f64, points: usize) {
    if !ptr.is_null() && points <= MAX_POINTS {
        drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(ptr, points * 2 + 2)));
    }
}

/// # Safety
/// ptr must reference a live allocation of points*2+2 f64 values.
#[no_mangle]
pub unsafe extern "C" fn path_metrics(ptr: *mut f64, points: usize) -> i32 {
    if ptr.is_null() || points > MAX_POINTS { return 0; }
    let data = std::slice::from_raw_parts_mut(ptr, points * 2 + 2);
    let mut length = 0.0;
    let mut area = 0.0;
    for i in 1..points {
        let a = (i - 1) * 2;
        let b = i * 2;
        let p1 = data[a + 1] * RAD;
        let p2 = data[b + 1] * RAD;
        let dl = (data[b] - data[a]) * RAD;
        let h = ((p2 - p1) / 2.0).sin().powi(2)
            + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
        length += 2.0 * R * h.sqrt().min(1.0).asin();
        area += dl * (2.0 + p1.sin() + p2.sin());
    }
    data[points * 2] = length;
    data[points * 2 + 1] = (area * R * R / 2.0).abs();
    1
}
